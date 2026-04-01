pub mod error_codes;
mod expr;
mod imports;
mod items;
mod match_check;
#[cfg(test)]
mod tests;
mod traits;
mod type_compat;
mod type_registration;
mod type_resolve;
mod types;

pub use error_codes::ErrorCode;
pub use types::{Type, TypeDisplay};

use std::collections::{HashMap, HashSet};

use crate::parser::ast::ExprId;

/// Maps expression IDs to their resolved types.
pub type ExprTypeMap = HashMap<ExprId, Type>;

/// Annotate every `Expr` in the program with its resolved type from the type map.
pub fn annotate_types(program: &mut Program, types: &ExprTypeMap) {
    crate::walk::walk_program_mut(program, &mut |expr| {
        if let Some(ty) = types.get(&expr.id) {
            expr.ty = ty.clone();
        }
    });
}

use crate::diagnostic::Diagnostic;
use crate::interop::{self, DtsExport};
use crate::lexer::span::Span;
use crate::parser::ast::*;
use crate::resolve::ResolvedImports;
use crate::stdlib::StdlibRegistry;
use crate::type_layout;
use types::{TypeEnv, TypeInfo};

// ── Context flags ────────────────────────────────────────────────

/// Transient context flags that change during recursive expression checking.
/// Bundled together so they can be saved/restored as a unit via `with_context`.
#[derive(Clone, Default)]
pub(crate) struct CheckContext {
    /// The return type of the current function (for ? validation).
    pub current_return_type: Option<Type>,
    /// Whether we are currently inside a `try` expression.
    pub inside_try: bool,
    /// Whether we are currently inside a `collect` block.
    pub inside_collect: bool,
    /// The error type collected from `?` operations inside a `collect` block.
    pub collect_err_type: Option<Type>,
    /// Whether we are checking an event handler prop value (onChange, onClick, etc.)
    pub event_handler_context: bool,
    /// Whether we are currently inside an `async` function.
    pub inside_async: bool,
    /// Hints for lambda parameter type inference from calling context.
    /// Each element corresponds to a parameter position (index 0 = first param, etc.).
    pub lambda_param_hints: Vec<Type>,
    /// When inside a pipe, holds the type of the piped (left) value.
    pub pipe_input_type: Option<Type>,
}

// ── Unused name tracking ────────────────────────────────────────

/// Tracks used/defined/imported names for unused detection.
#[derive(Default)]
pub(crate) struct UnusedTracker {
    /// Variables/functions referenced.
    pub used_names: HashSet<String>,
    /// Defined variables with spans (for unused warnings).
    pub defined_names: Vec<(String, Span)>,
    /// Imported names with spans (for unused import errors).
    pub imported_names: Vec<(String, Span)>,
    /// Where each name was defined — for shadowing error messages.
    pub defined_sources: HashMap<String, String>,
}

// ── Trait registry ──────────────────────────────────────────────

/// Tracks trait declarations and implementations.
#[derive(Default)]
pub(crate) struct TraitRegistry {
    /// Registered trait declarations: trait name -> methods.
    pub trait_defs: HashMap<String, Vec<TraitMethodSig>>,
    /// Tracks which (type, trait) pairs have been implemented.
    pub trait_impls: HashSet<(String, String)>,
}

/// Check if an expression is wrapped in `await` (possibly through `try`).
pub fn expr_has_await(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Await(_) => true,
        ExprKind::Try(inner) => expr_has_await(inner),
        _ => false,
    }
}

// ── Checker ──────────────────────────────────────────────────────

/// The Floe type checker.
pub struct Checker {
    env: TypeEnv,
    diagnostics: Vec<Diagnostic>,
    next_var: usize,
    /// Standard library function registry.
    stdlib: StdlibRegistry,
    /// Maps expression IDs to their resolved types.
    /// Used by codegen for type-directed pipe resolution.
    expr_types: ExprTypeMap,
    /// Context flags for the current checking position.
    pub(crate) ctx: CheckContext,
    /// Unused name tracking.
    pub(crate) unused: UnusedTracker,
    /// Trait declarations and implementations.
    pub(crate) traits: TraitRegistry,
    /// Names of untrusted (external TS) imports that require `try`.
    untrusted_imports: HashSet<String>,
    /// Whether we are in the type registration pass (suppress unknown type errors).
    registering_types: bool,
    /// Pre-resolved imports from other .fl files, keyed by import source string.
    resolved_imports: HashMap<String, ResolvedImports>,
    /// Pre-resolved .d.ts exports for npm imports, keyed by specifier (e.g. "react").
    dts_imports: HashMap<String, Vec<DtsExport>>,
    /// Tracks consumed probe exports to prevent reuse.
    /// Uses (specifier_name, export_index) pairs for stable identification
    /// across HashMap iteration orders.
    probe_consumed: HashSet<(String, usize)>,
    /// Maps variable/function names to their inferred type display names.
    /// Accumulated as names are defined so inner-scope names aren't lost.
    name_types: HashMap<String, String>,
    /// Variant names that appear in multiple unions: variant name -> list of union names.
    /// Used to detect ambiguous bare variant usage.
    ambiguous_variants: HashMap<String, Vec<String>>,
    /// Maps function names to their required (non-default) parameter count.
    /// Functions not in this map require all parameters.
    fn_required_params: HashMap<String, usize>,
    /// Maps function names to their parameter names (for validating named arguments).
    fn_param_names: HashMap<String, Vec<String>>,
    /// Maps for-block function names to all overloads: (receiver_type_name, fn_type).
    /// Used to resolve the correct overload when multiple for-blocks define the same function name,
    /// and to detect redefinition conflicts (same name on same type).
    for_block_overloads: HashMap<String, Vec<(String, Type)>>,
    /// Maps component_name -> prop_name -> resolved callback parameter type.
    /// Populated from tsgo probe results for JSX props with arrow function values.
    jsx_callback_hints: HashMap<String, HashMap<String, Type>>,
    /// Maps component_name -> Vec of parameter types for children render props.
    /// Populated from tsgo probe results (__jsxc_Component_N entries).
    jsx_children_hints: HashMap<String, Vec<Type>>,
}

/// Signature of a trait method (for checking implementations).
#[derive(Debug, Clone)]
pub(crate) struct TraitMethodSig {
    pub name: String,
    /// Whether this method has a default implementation.
    pub has_default: bool,
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

impl Checker {
    pub fn new() -> Self {
        let mut env = TypeEnv::new();

        // ── Built-in runtime types ──────────────────────────────────
        //
        // These are web/JS standard types that Floe code can use without
        // importing. Defined as Records so member access works through
        // the normal type-checking path.

        let response_record = Type::Record(vec![
            (
                "json".to_string(),
                Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Unknown),
                    required_params: 0,
                },
            ),
            (
                "text".to_string(),
                Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::String),
                    required_params: 0,
                },
            ),
            ("ok".to_string(), Type::Bool),
            ("status".to_string(), Type::Number),
            ("statusText".to_string(), Type::String),
            ("headers".to_string(), Type::Named("Headers".to_string())),
            ("url".to_string(), Type::String),
        ]);

        let error_record = Type::Record(vec![
            ("message".to_string(), Type::String),
            ("name".to_string(), Type::String),
            ("stack".to_string(), Type::option_of(Type::String)),
        ]);

        let event_record = Type::Record(vec![
            (
                "target".to_string(),
                Type::Record(vec![
                    ("value".to_string(), Type::String),
                    ("checked".to_string(), Type::Bool),
                ]),
            ),
            ("key".to_string(), Type::String),
            ("code".to_string(), Type::String),
            (
                "preventDefault".to_string(),
                Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Unit),
                    required_params: 0,
                },
            ),
            (
                "stopPropagation".to_string(),
                Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Unit),
                    required_params: 0,
                },
            ),
        ]);

        // Register as named types that display nicely and resolve to
        // records for member access via resolve_type_to_concrete
        env.define("Response", response_record);
        env.define("Error", error_record);
        env.define("Event", event_record);

        // Register Option variant names so Some/None resolve as variants
        let option_unknown = Type::option_of(Type::Unknown);
        env.define(type_layout::VARIANT_SOME, option_unknown.clone());
        env.define(type_layout::VARIANT_NONE, option_unknown);

        // Register Result variant names so Ok/Err resolve as variants
        let result_unknown = Type::result_of(Type::Unknown, Type::Unknown);
        env.define(type_layout::VARIANT_OK, result_unknown.clone());
        env.define(type_layout::VARIANT_ERR, result_unknown);

        // ── Browser/runtime globals ─────────────────────────────────

        let browser_globals: &[(&str, Type)] = &[
            (
                "fetch",
                Type::Function {
                    params: vec![Type::String],
                    return_type: Box::new(Type::Promise(Box::new(Type::Named(
                        "Response".to_string(),
                    )))),
                    required_params: 1,
                },
            ),
            ("window", Type::Unknown),
            ("document", Type::Unknown),
            (
                "setTimeout",
                Type::Function {
                    params: vec![
                        Type::Function {
                            params: vec![],
                            return_type: Box::new(Type::Unit),
                            required_params: 0,
                        },
                        Type::Number,
                    ],
                    return_type: Box::new(Type::Number),
                    required_params: 2,
                },
            ),
            (
                "setInterval",
                Type::Function {
                    params: vec![
                        Type::Function {
                            params: vec![],
                            return_type: Box::new(Type::Unit),
                            required_params: 0,
                        },
                        Type::Number,
                    ],
                    return_type: Box::new(Type::Number),
                    required_params: 2,
                },
            ),
            (
                "clearTimeout",
                Type::Function {
                    params: vec![Type::Number],
                    return_type: Box::new(Type::Unit),
                    required_params: 1,
                },
            ),
            (
                "clearInterval",
                Type::Function {
                    params: vec![Type::Number],
                    return_type: Box::new(Type::Unit),
                    required_params: 1,
                },
            ),
            ("Promise", Type::Unknown),
            ("JSON", Type::Unknown),
        ];

        for (name, ty) in browser_globals {
            env.define(name, ty.clone());
        }

        // Browser globals that can throw and require `try`
        let mut untrusted_globals = HashSet::new();
        untrusted_globals.insert("fetch".to_string());

        Self {
            env,
            diagnostics: Vec::new(),
            next_var: 0,
            stdlib: StdlibRegistry::new(),
            expr_types: HashMap::new(),
            ctx: CheckContext::default(),
            unused: UnusedTracker::default(),
            traits: TraitRegistry::default(),
            untrusted_imports: untrusted_globals,
            registering_types: false,
            resolved_imports: HashMap::new(),
            dts_imports: HashMap::new(),
            probe_consumed: HashSet::new(),
            name_types: HashMap::new(),
            ambiguous_variants: HashMap::new(),
            fn_required_params: HashMap::new(),
            fn_param_names: HashMap::new(),
            for_block_overloads: HashMap::new(),
            jsx_callback_hints: HashMap::new(),
            jsx_children_hints: HashMap::new(),
        }
    }

    /// Create a checker with pre-resolved imports from other .fl files.
    pub fn with_imports(imports: HashMap<String, ResolvedImports>) -> Self {
        Self {
            resolved_imports: imports,
            ..Self::new()
        }
    }

    /// Create a checker with both .fl and .d.ts imports.
    pub fn with_all_imports(
        fl_imports: HashMap<String, ResolvedImports>,
        dts_imports: HashMap<String, Vec<DtsExport>>,
    ) -> Self {
        Self {
            resolved_imports: fl_imports,
            dts_imports,
            ..Self::new()
        }
    }

    /// Run `f` with modified context flags, then restore the previous context.
    /// This replaces manual save/restore patterns like:
    /// ```ignore
    /// let prev = self.ctx.inside_try;
    /// self.ctx.inside_try = true;
    /// // ... work ...
    /// self.ctx.inside_try = prev;
    /// ```
    pub(crate) fn with_context<T>(
        &mut self,
        modify: impl FnOnce(&mut CheckContext),
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let saved = self.ctx.clone();
        modify(&mut self.ctx);
        let result = f(self);
        self.ctx = saved;
        result
    }

    /// Push a new scope, run `f`, then pop the scope.
    pub(crate) fn in_scope<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.env.push_scope();
        let result = f(self);
        self.env.pop_scope();
        result
    }

    /// Check a program and return diagnostics.
    pub fn check(self, program: &Program) -> Vec<Diagnostic> {
        self.check_full(program).0
    }

    /// Check a program and return (diagnostics, expr_type_map).
    /// The expr_type_map maps expression spans (start, end) to their resolved types,
    /// used by codegen for type-directed pipe resolution.
    pub fn check_full(self, program: &Program) -> (Vec<Diagnostic>, ExprTypeMap) {
        let (diags, _, expr_types) = self.check_all(program);
        (diags, expr_types)
    }

    /// Check a program and return diagnostics, name_type_map, and expr_type_map.
    /// The name_type_map maps variable/function names to their inferred type display names.
    /// The expr_type_map maps ExprId to resolved Type (used by annotate_types).
    pub fn check_with_types(
        self,
        program: &Program,
    ) -> (Vec<Diagnostic>, HashMap<String, String>, ExprTypeMap) {
        self.check_all(program)
    }

    /// Internal: run all checks and return all maps.
    #[allow(clippy::type_complexity)]
    fn check_all(
        mut self,
        program: &Program,
    ) -> (Vec<Diagnostic>, HashMap<String, String>, ExprTypeMap) {
        // Pre-register types, traits, and functions from resolved imports
        self.registering_types = true;
        // Register foreign (npm) type names first so they're in scope when
        // resolving fields of imported type declarations
        for resolved in self.resolved_imports.values() {
            for name in &resolved.foreign_type_names {
                self.env.define(name, Type::Foreign(name.clone()));
            }
        }
        for resolved in self.resolved_imports.values().cloned().collect::<Vec<_>>() {
            for decl in &resolved.type_decls {
                // Skip naming checks for imported types (already validated in source)
                self.register_type_decl(decl, Span::new(0, 0, 0, 0));
            }
            for decl in &resolved.trait_decls {
                self.register_trait_decl(decl);
            }
        }
        self.registering_types = false;

        // Register functions from resolved imports
        for (source, resolved) in self
            .resolved_imports
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>()
        {
            for func in &resolved.function_decls {
                let return_type = func
                    .return_type
                    .as_ref()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                let param_types: Vec<_> = func
                    .params
                    .iter()
                    .map(|p| {
                        p.type_ann
                            .as_ref()
                            .map(|t| self.resolve_type(t))
                            .unwrap_or(Type::Unknown)
                    })
                    .collect();
                // Track required (non-default) parameter count
                let required_params = func.params.iter().filter(|p| p.default.is_none()).count();
                let fn_type = Type::Function {
                    params: param_types,
                    return_type: Box::new(return_type),
                    required_params,
                };
                self.env.define(&func.name, fn_type);
                self.unused
                    .defined_sources
                    .insert(func.name.clone(), format!("function from \"{}\"", source));
                if required_params < func.params.len() {
                    self.fn_required_params
                        .insert(func.name.clone(), required_params);
                }

                // Track parameter names for named argument validation
                self.fn_param_names.insert(
                    func.name.clone(),
                    func.params.iter().map(|p| p.name.clone()).collect(),
                );
            }
        }

        // First pass: register all type declarations and traits
        self.registering_types = true;
        for item in &program.items {
            match &item.kind {
                ItemKind::TypeDecl(decl) => {
                    self.register_type_decl(decl, item.span);
                }
                ItemKind::TraitDecl(decl) => {
                    self.register_trait_decl(decl);
                }
                _ => {}
            }
        }
        self.registering_types = false;

        // Build JSX callback hints from tsgo probe results (__jsx_Component_prop entries)
        // and children render prop hints (__jsxc_Component_N entries)
        for exports in self.dts_imports.values() {
            for export in exports {
                if let Some(rest) = export.name.strip_prefix("__jsxc_") {
                    if let Some(sep) = rest.rfind('_') {
                        let component = &rest[..sep];
                        let index: usize = match rest[sep + 1..].parse() {
                            Ok(i) => i,
                            Err(_) => continue,
                        };
                        let ty = interop::wrap_boundary_type(&export.ts_type);
                        if !matches!(ty, Type::Unknown | Type::Never) {
                            let params = self
                                .jsx_children_hints
                                .entry(component.to_string())
                                .or_default();
                            if index >= params.len() {
                                params.resize(index + 1, Type::Unknown);
                            }
                            params[index] = ty;
                        }
                    }
                } else if let Some(rest) = export.name.strip_prefix("__jsx_")
                    && let Some(sep) = rest.find('_')
                {
                    let component = &rest[..sep];
                    let prop = &rest[sep + 1..];
                    let ty = interop::wrap_boundary_type(&export.ts_type);
                    if !matches!(ty, Type::Unknown | Type::Never) {
                        self.jsx_callback_hints
                            .entry(component.to_string())
                            .or_default()
                            .insert(prop.to_string(), ty);
                    }
                }
            }
        }

        // Second pass: check all items
        for item in &program.items {
            self.check_item(item);
        }

        // Check for unused imports
        for (name, span) in &self.unused.imported_names {
            if !self.unused.used_names.contains(name) {
                self.diagnostics.push(
                    Diagnostic::error(format!("unused import `{name}`"), *span)
                        .with_label("imported but never used")
                        .with_help("remove this import or use it in the code")
                        .with_error_code(ErrorCode::UnusedImport),
                );
            }
        }

        // Check for unused variables
        for (name, span) in &self.unused.defined_names {
            if !name.starts_with('_') && !self.unused.used_names.contains(name) {
                self.diagnostics.push(
                    Diagnostic::warning(format!("unused variable `{name}`"), *span)
                        .with_label("defined but never used")
                        .with_help(format!("prefix with underscore `_{name}` to suppress"))
                        .with_code("W001"),
                );
            }
        }

        // Merge any remaining scope entries into name_types
        for scope in &self.env.scopes {
            for (name, ty) in scope {
                self.name_types
                    .entry(name.clone())
                    .or_insert_with(|| ty.to_string());
            }
        }

        (self.diagnostics, self.name_types, self.expr_types)
    }

    // ── Diagnostic helpers ────────────────────────────────────────

    fn emit_error(
        &mut self,
        msg: impl Into<String>,
        span: Span,
        code: ErrorCode,
        label: impl Into<String>,
    ) {
        self.diagnostics.push(
            Diagnostic::error(msg, span)
                .with_label(label)
                .with_error_code(code),
        );
    }

    fn emit_error_with_help(
        &mut self,
        msg: impl Into<String>,
        span: Span,
        code: ErrorCode,
        label: impl Into<String>,
        help: impl Into<String>,
    ) {
        self.diagnostics.push(
            Diagnostic::error(msg, span)
                .with_label(label)
                .with_help(help)
                .with_error_code(code),
        );
    }

    fn emit_warning_with_help(
        &mut self,
        msg: impl Into<String>,
        span: Span,
        code: ErrorCode,
        label: impl Into<String>,
        help: impl Into<String>,
    ) {
        self.diagnostics.push(
            Diagnostic::warning(msg, span)
                .with_label(label)
                .with_help(help)
                .with_error_code(code),
        );
    }

    // ── Type helpers ────────────────────────────────────────────────

    fn fresh_type_var(&mut self) -> Type {
        let id = self.next_var;
        self.next_var += 1;
        Type::Var(id)
    }

    /// Emit an error if `name` is already defined in any scope (no shadowing allowed).
    /// When tsgo loses Option<T> through useState type inference (because TS
    /// collapses FloeOption<T> to T), reconstruct the correct types from the
    /// original call's type arguments.
    fn correct_usestate_option_type(&mut self, tsgo_type: &Type, value: &Expr) -> Option<Type> {
        // Only applies to Tuple types from array destructuring
        let Type::Tuple(tsgo_elems) = tsgo_type else {
            return None;
        };
        if tsgo_elems.len() != 2 {
            return None;
        }

        // Check if the value is a call with Option<T> type arg
        let ExprKind::Call { type_args, .. } = &value.kind else {
            return None;
        };
        if type_args.len() != 1 {
            return None;
        }

        // Check if the type arg is Option<T>
        let type_arg = &type_args[0];
        if let TypeExprKind::Named {
            name,
            type_args: inner_args,
            ..
        } = &type_arg.kind
            && name == type_layout::TYPE_OPTION
            && inner_args.len() == 1
        {
            let option_type = self.resolve_type(type_arg);
            // Replace: [T, (T) -> ()] → [Option<T>, (Option<T>) -> ()]
            return Some(Type::Tuple(vec![
                option_type.clone(),
                Type::Function {
                    params: vec![option_type],
                    return_type: Box::new(Type::Unit),
                    required_params: 1,
                },
            ]));
        }

        None
    }

    fn check_no_redefinition(&mut self, name: &str, span: Span) {
        // Allow shadowing from outer scopes (like Rust/Gleam) — only reject
        // duplicate definitions within the same scope.
        if self.env.is_defined_in_current_scope(name) {
            let msg = format!("`{name}` is already defined in this scope");
            self.emit_error(msg, span, ErrorCode::DuplicateDefinition, "already defined");
        }
    }
}
