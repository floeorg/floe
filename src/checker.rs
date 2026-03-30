mod expr;
mod match_check;
#[cfg(test)]
mod tests;
mod types;

pub use types::Type;

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
    /// Hint for lambda parameter type inference from calling context.
    pub lambda_param_hint: Option<Type>,
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
                },
            ),
            (
                "text".to_string(),
                Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::String),
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
            ("stack".to_string(), Type::Option(Box::new(Type::String))),
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
                },
            ),
            (
                "stopPropagation".to_string(),
                Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Unit),
                },
            ),
        ]);

        // Register as named types that display nicely and resolve to
        // records for member access via resolve_type_to_concrete
        env.define("Response", response_record);
        env.define("Error", error_record);
        env.define("Event", event_record);

        // ── Browser/runtime globals ─────────────────────────────────

        let browser_globals: &[(&str, Type)] = &[
            (
                "fetch",
                Type::Function {
                    params: vec![Type::String],
                    return_type: Box::new(Type::Promise(Box::new(Type::Named(
                        "Response".to_string(),
                    )))),
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
                        },
                        Type::Number,
                    ],
                    return_type: Box::new(Type::Number),
                },
            ),
            (
                "setInterval",
                Type::Function {
                    params: vec![
                        Type::Function {
                            params: vec![],
                            return_type: Box::new(Type::Unit),
                        },
                        Type::Number,
                    ],
                    return_type: Box::new(Type::Number),
                },
            ),
            (
                "clearTimeout",
                Type::Function {
                    params: vec![Type::Number],
                    return_type: Box::new(Type::Unit),
                },
            ),
            (
                "clearInterval",
                Type::Function {
                    params: vec![Type::Number],
                    return_type: Box::new(Type::Unit),
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
                let fn_type = Type::Function {
                    params: param_types,
                    return_type: Box::new(return_type),
                };
                self.env.define(&func.name, fn_type);
                self.unused
                    .defined_sources
                    .insert(func.name.clone(), format!("function from \"{}\"", source));

                // Track required (non-default) parameter count
                let required_params = func.params.iter().filter(|p| p.default.is_none()).count();
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
                        .with_code("E009"),
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
                    .or_insert_with(|| ty.display_name());
            }
        }

        (self.diagnostics, self.name_types, self.expr_types)
    }

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
            self.diagnostics.push(
                Diagnostic::error(msg, span)
                    .with_label("already defined")
                    .with_code("E016"),
            );
        }
    }

    // ── Type Registration ────────────────────────────────────────

    fn register_type_decl(&mut self, decl: &TypeDecl, span: Span) {
        // Enforce naming conventions
        if span.start != 0 || span.end != 0 {
            // Only check local declarations (not imports with dummy spans)
            if decl.name.starts_with(char::is_lowercase) {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "type name `{}` must start with an uppercase letter",
                            decl.name
                        ),
                        span,
                    )
                    .with_label("must be uppercase")
                    .with_help(format!(
                        "rename to `{}{}`",
                        decl.name[..1].to_uppercase(),
                        &decl.name[1..]
                    ))
                    .with_code("E024"),
                );
            }
            match &decl.def {
                TypeDef::Union(variants) => {
                    for variant in variants {
                        if variant.name.starts_with(char::is_lowercase) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!(
                                        "variant name `{}` must start with an uppercase letter",
                                        variant.name
                                    ),
                                    variant.span,
                                )
                                .with_help(format!(
                                    "rename to `{}{}`",
                                    variant.name[..1].to_uppercase(),
                                    &variant.name[1..]
                                ))
                                .with_code("E024"),
                            );
                        }
                    }
                }
                // Record field names: uppercase fields are already rejected by the parser
                // (uppercase identifiers are parsed as types/variants, not field names)
                TypeDef::Record(_) => {}
                _ => {}
            }

            // Reject & intersection in { } type definitions (records/unions).
            // & should only appear in = type aliases (TS bridge types).
            if matches!(decl.def, TypeDef::Record(_) | TypeDef::Union(_)) {
                self.check_no_intersection_in_type_def(&decl.def, span);
            }
        }

        // Flatten record spreads into a flat record definition
        let flattened_def = self.flatten_record_spreads(&decl.def, &decl.name);

        let info = TypeInfo {
            def: flattened_def.clone(),
            opaque: decl.opaque,
            type_params: decl.type_params.clone(),
        };
        self.env.define_type(&decl.name, info);

        // Register the type name in the value namespace too (for constructors)
        match &flattened_def {
            TypeDef::Record(entries) => {
                // If the record has unresolved spreads (foreign generic types),
                // use the type probe for the value namespace so member access
                // resolves through the foreign type system.
                let has_unresolved_spreads =
                    entries.iter().any(|e| matches!(e, RecordEntry::Spread(_)));
                if has_unresolved_spreads {
                    if let Some(probe_ty) = self.find_type_probe(&decl.name) {
                        self.env.define(&decl.name, probe_ty);
                    } else {
                        self.env.define(&decl.name, Type::Named(decl.name.clone()));
                    }
                } else {
                    self.env.define(&decl.name, Type::Named(decl.name.clone()));
                }

                // Populate __field_ entries for dot-access completions
                for entry in entries {
                    if let RecordEntry::Field(field) = entry {
                        let field_ty = self.resolve_type(&field.type_ann);
                        self.name_types.insert(
                            format!("__field_{}_{}", decl.name, field.name),
                            field_ty.display_name(),
                        );
                    }
                }
            }
            TypeDef::Union(variants) => {
                let var_types: Vec<_> = variants
                    .iter()
                    .map(|v| {
                        let field_types: Vec<_> = v
                            .fields
                            .iter()
                            .map(|f| self.resolve_type(&f.type_ann))
                            .collect();
                        (v.name.clone(), field_types)
                    })
                    .collect();
                let union_type = Type::Union {
                    name: decl.name.clone(),
                    variants: var_types.clone(),
                };
                self.env.define(&decl.name, union_type.clone());
                // Register each variant constructor and track ambiguity
                for (vname, _) in &var_types {
                    // Check if this variant name is already defined by another union
                    if let Some(existing) = self.env.lookup(vname)
                        && let Type::Union {
                            name: existing_union,
                            ..
                        } = existing
                        && *existing_union != decl.name
                    {
                        let existing_union = existing_union.clone();
                        self.ambiguous_variants
                            .entry(vname.clone())
                            .or_insert_with(|| vec![existing_union])
                            .push(decl.name.clone());
                    }
                    self.env.define(vname, union_type.clone());
                }
            }
            TypeDef::StringLiteralUnion(variants) => {
                let ty = Type::StringLiteralUnion {
                    name: decl.name.clone(),
                    variants: variants.clone(),
                };
                self.env.define(&decl.name, ty);
            }
            TypeDef::Alias(type_expr) => {
                let mut ty = self.resolve_type(type_expr);
                // If tsgo resolved a type probe for this alias, use it.
                // This handles conditional/mapped types from npm packages
                // (e.g. VariantProps<T> which uses Extract<...> internally).
                if let Some(probe_ty) = self.find_type_probe(&decl.name) {
                    ty = probe_ty;
                }
                self.env.define(&decl.name, ty);
            }
        }
    }

    /// Search dts_imports for a tsgo type probe matching the given type alias name.
    fn find_type_probe(&self, type_name: &str) -> Option<Type> {
        let probe_key = format!("__tprobe_{type_name}");
        for exports in self.dts_imports.values() {
            for export in exports {
                if export.name == probe_key {
                    return Some(interop::wrap_boundary_type(&export.ts_type));
                }
            }
        }
        None
    }

    /// Check that `&` intersection types don't appear in `{ }` type definitions.
    fn check_no_intersection_in_type_def(&mut self, def: &TypeDef, span: Span) {
        fn has_intersection(ty: &TypeExpr) -> bool {
            match &ty.kind {
                TypeExprKind::Intersection(_) => true,
                TypeExprKind::Array(inner) => has_intersection(inner),
                TypeExprKind::Tuple(parts) => parts.iter().any(has_intersection),
                TypeExprKind::Function {
                    params,
                    return_type,
                } => params.iter().any(has_intersection) || has_intersection(return_type),
                TypeExprKind::Named { type_args, .. } => type_args.iter().any(has_intersection),
                TypeExprKind::Record(fields) => {
                    fields.iter().any(|f| has_intersection(&f.type_ann))
                }
                _ => false,
            }
        }

        let found = match def {
            TypeDef::Record(entries) => entries
                .iter()
                .filter_map(|e| e.as_field().map(|f| &f.type_ann))
                .any(has_intersection),
            TypeDef::Union(variants) => variants
                .iter()
                .flat_map(|v| v.fields.iter().map(|f| &f.type_ann))
                .any(has_intersection),
            _ => return,
        };

        if found {
            self.diagnostics.push(
                Diagnostic::error(
                    "`&` intersection types cannot be used in `{ }` type definitions".to_string(),
                    span,
                )
                .with_help("use `...Spread` for record composition, or `=` for TS interop types")
                .with_code("E025"),
            );
        }
    }

    /// Flatten record type spreads (`...OtherType`) into regular fields.
    /// Returns the original `TypeDef` unchanged if it's not a record or has no spreads.
    fn flatten_record_spreads(&mut self, def: &TypeDef, type_name: &str) -> TypeDef {
        let entries = match def {
            TypeDef::Record(entries) => entries,
            other => return other.clone(),
        };

        // Check if there are any spreads at all
        let has_spreads = entries.iter().any(|e| matches!(e, RecordEntry::Spread(_)));
        if !has_spreads {
            return def.clone();
        }

        let mut flat_fields: Vec<RecordField> = Vec::new();
        let mut preserved_spreads: Vec<RecordEntry> = Vec::new();
        let mut seen_names: std::collections::HashMap<String, Span> =
            std::collections::HashMap::new();

        for entry in entries {
            match entry {
                RecordEntry::Field(field) => {
                    if seen_names.contains_key(&field.name) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!(
                                    "duplicate field `{}` in record type `{}`",
                                    field.name, type_name
                                ),
                                field.span,
                            )
                            .with_label("duplicate field")
                            .with_help("field was already defined elsewhere in this record type")
                            .with_code("E030"),
                        );
                    } else {
                        seen_names.insert(field.name.clone(), field.span);
                        flat_fields.push(field.as_ref().clone());
                    }
                }
                RecordEntry::Spread(spread) => {
                    // Look up the referenced type
                    if let Some(info) = self.env.lookup_type(&spread.type_name) {
                        let info = info.clone();
                        match &info.def {
                            TypeDef::Record(spread_entries) => {
                                // Get only the direct fields from the spread target
                                // (which should already be flattened if it was registered first)
                                let spread_fields: Vec<RecordField> = spread_entries
                                    .iter()
                                    .filter_map(|e| e.as_field().cloned())
                                    .collect();
                                for field in &spread_fields {
                                    if seen_names.contains_key(&field.name) {
                                        self.diagnostics.push(
                                            Diagnostic::error(
                                                format!(
                                                    "field `{}` from spread `...{}` conflicts with existing field in `{}`",
                                                    field.name, spread.type_name, type_name
                                                ),
                                                spread.span,
                                            )
                                            .with_label(format!("field `{}` already defined", field.name))
                                            .with_help("field was already defined elsewhere in this record type")
                                            .with_code("E031"),
                                        );
                                    } else {
                                        seen_names.insert(field.name.clone(), spread.span);
                                        flat_fields.push(field.clone());
                                    }
                                }
                            }
                            TypeDef::Union(_) => {
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        format!(
                                            "cannot spread union type `{}` into record type `{}`",
                                            spread.type_name, type_name
                                        ),
                                        spread.span,
                                    )
                                    .with_label("spread target must be a record type")
                                    .with_code("E032"),
                                );
                            }
                            TypeDef::Alias(_) | TypeDef::StringLiteralUnion(_) => {
                                // If the alias is a foreign type, preserve the spread for codegen
                                preserved_spreads.push(RecordEntry::Spread(spread.clone()));
                            }
                        }
                    } else if spread.type_expr.is_some() {
                        // Foreign/generic type not in local env — preserve for codegen
                        // (e.g. ...VariantProps<typeof cardVariants>)
                        self.unused.used_names.insert(spread.type_name.clone());
                        preserved_spreads.push(RecordEntry::Spread(spread.clone()));
                    } else {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!("unknown type `{}` in spread", spread.type_name),
                                spread.span,
                            )
                            .with_label("type not found")
                            .with_code("E002"),
                        );
                    }
                }
            }
        }

        let mut result_entries: Vec<RecordEntry> = preserved_spreads;
        result_entries.extend(
            flat_fields
                .into_iter()
                .map(|f| RecordEntry::Field(Box::new(f))),
        );

        TypeDef::Record(result_entries)
    }

    /// Second-pass validation of type annotations within type declarations.
    /// The first pass (register_type_decl) skips unknown type errors for forward references.
    fn validate_type_decl_annotations(&mut self, decl: &TypeDecl) {
        match &decl.def {
            TypeDef::Record(entries) => {
                let mut seen_default = false;
                for entry in entries {
                    if let RecordEntry::Field(field) = entry {
                        let field_ty = self.resolve_type(&field.type_ann);
                        if let Some(ref default_expr) = field.default {
                            seen_default = true;
                            let default_ty = self.check_expr(default_expr);
                            if !self.types_compatible(&field_ty, &default_ty) {
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        format!(
                                            "default value for `{}`: expected `{}`, found `{}`",
                                            field.name,
                                            field_ty.display_name(),
                                            default_ty.display_name()
                                        ),
                                        field.span,
                                    )
                                    .with_label(format!("expected `{}`", field_ty.display_name()))
                                    .with_code("E001"),
                                );
                            }
                        } else if seen_default {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!(
                                        "required field `{}` must come before fields with defaults",
                                        field.name
                                    ),
                                    field.span,
                                )
                                .with_label("move this field before defaulted fields")
                                .with_code("E001"),
                            );
                        }
                    }
                    // Spreads are validated during register_type_decl
                }
            }
            TypeDef::Union(variants) => {
                for variant in variants {
                    for field in &variant.fields {
                        self.resolve_type(&field.type_ann);
                    }
                }
            }
            TypeDef::StringLiteralUnion(_) => {
                // No type annotations to validate
            }
            TypeDef::Alias(type_expr) => {
                let ty = self.resolve_type(type_expr);
                // typeof aliases resolved to Unknown in the first pass (bindings
                // weren't registered yet). Now that bindings exist, update the env.
                if matches!(type_expr.kind, TypeExprKind::TypeOf(_)) {
                    self.env.define(&decl.name, ty);
                }
            }
        }

        // Validate and register deriving clause
        if !decl.deriving.is_empty() {
            self.check_deriving(decl);
        }
    }

    /// Validate a `deriving` clause and register the derived functions.
    fn check_deriving(&mut self, decl: &TypeDecl) {
        let span = Span::new(0, 0, 0, 0); // deriving doesn't have its own span yet

        // deriving only works on record types
        if !matches!(&decl.def, TypeDef::Record(_)) {
            self.diagnostics.push(
                Diagnostic::error(
                    format!(
                        "`deriving` can only be used on record types, but `{}` is not a record",
                        decl.name
                    ),
                    span,
                )
                .with_label("not a record type")
                .with_help("remove the `deriving` clause or change this to a record type")
                .with_code("E019"),
            );
            return;
        }

        let type_name = &decl.name;

        for trait_name in &decl.deriving {
            match trait_name.as_str() {
                "Eq" => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "`Eq` cannot be derived — structural equality is built-in for all types via `==`".to_string(),
                            span,
                        )
                        .with_label("not needed")
                        .with_help("remove `Eq` from the deriving clause — use `==` for equality comparison")
                        .with_code("E019"),
                    );
                }
                "Display" => {
                    // Register display function: fn display(self) -> string
                    let fn_name = "display".to_string();
                    let self_type = Type::Named(type_name.clone());
                    let fn_type = Type::Function {
                        params: vec![self_type],
                        return_type: Box::new(Type::String),
                    };
                    self.env.define(&fn_name, fn_type);
                    self.unused
                        .defined_sources
                        .insert(fn_name.clone(), format!("derived Display for {type_name}"));
                    self.unused.used_names.insert(fn_name.clone());
                    self.traits
                        .trait_impls
                        .insert((type_name.clone(), "Display".to_string()));
                }
                _ => {
                    self.diagnostics.push(
                        Diagnostic::error(format!("trait `{trait_name}` cannot be derived"), span)
                            .with_label("not a derivable trait")
                            .with_help("only `Display` can be derived")
                            .with_code("E019"),
                    );
                }
            }

            // Mark the trait name as used
            self.unused.used_names.insert(trait_name.clone());
        }
    }

    fn resolve_type(&mut self, type_expr: &TypeExpr) -> Type {
        match &type_expr.kind {
            TypeExprKind::Named {
                name,
                type_args,
                bounds,
            } => {
                // Store bounds information for later trait bound checking
                if !bounds.is_empty() {
                    self.env.define_type_param_bounds(name, bounds.clone());
                }
                self.resolve_named_type(name, type_args, type_expr.span)
            }
            TypeExprKind::Record(fields) => {
                let field_types: Vec<_> = fields
                    .iter()
                    .map(|f| (f.name.clone(), self.resolve_type(&f.type_ann)))
                    .collect();
                Type::Record(field_types)
            }
            TypeExprKind::Function {
                params,
                return_type,
            } => {
                let param_types: Vec<_> = params.iter().map(|p| self.resolve_type(p)).collect();
                let ret = self.resolve_type(return_type);
                Type::Function {
                    params: param_types,
                    return_type: Box::new(ret),
                }
            }
            TypeExprKind::Array(inner) => Type::Array(Box::new(self.resolve_type(inner))),
            TypeExprKind::Tuple(types) => {
                Type::Tuple(types.iter().map(|t| self.resolve_type(t)).collect())
            }
            TypeExprKind::Intersection(types) => {
                // Resolve each member and merge into a single Record if all are records,
                // otherwise keep as the first resolved type (best-effort for npm interop)
                let resolved: Vec<Type> = types.iter().map(|t| self.resolve_type(t)).collect();
                let mut fields = Vec::new();
                let mut all_records = true;
                let mut first = None;
                for ty in &resolved {
                    let concrete = self
                        .env
                        .resolve_to_concrete(ty, &expr::simple_resolve_type_expr);
                    if let Type::Record(f) = concrete {
                        fields.extend(f);
                    } else {
                        all_records = false;
                        if first.is_none() {
                            first = Some(ty.clone());
                        }
                    }
                }
                if all_records && !fields.is_empty() {
                    Type::Record(fields)
                } else {
                    first.unwrap_or_else(|| resolved.into_iter().next().unwrap_or(Type::Unknown))
                }
            }
            TypeExprKind::StringLiteral(value) => Type::Foreign(format!("\"{value}\"")),
            TypeExprKind::TypeOf(name) => {
                let root = name.split('.').next().unwrap_or(name);
                self.unused.used_names.insert(root.to_string());

                // Bindings aren't registered yet during the first pass — defer to second pass
                if self.registering_types {
                    return Type::Unknown;
                }

                if let Some(ty) = self.env.lookup(name) {
                    ty.clone()
                } else {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!("cannot use `typeof` on undefined binding `{name}`"),
                            type_expr.span,
                        )
                        .with_label("not defined")
                        .with_help("typeof can only be used with value bindings (const, fn)")
                        .with_code("E002"),
                    );
                    Type::Unknown
                }
            }
        }
    }

    fn resolve_named_type(&mut self, name: &str, type_args: &[TypeExpr], span: Span) -> Type {
        // Mark type names as used (e.g. "JSX" from "JSX.Element", or "User")
        let root = name.split('.').next().unwrap_or(name);
        self.unused.used_names.insert(root.to_string());

        match name {
            type_layout::TYPE_NUMBER => Type::Number,
            type_layout::TYPE_STRING => Type::String,
            type_layout::TYPE_BOOLEAN => Type::Bool,
            type_layout::TYPE_UNIT => Type::Unit,
            type_layout::TYPE_UNDEFINED => Type::Undefined,
            type_layout::TYPE_UNKNOWN => Type::Unknown,
            type_layout::TYPE_ERROR | type_layout::TYPE_RESPONSE => Type::Named(name.to_string()),
            type_layout::TYPE_RESULT => {
                let ok = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                let err = type_args
                    .get(1)
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::Result {
                    ok: Box::new(ok),
                    err: Box::new(err),
                }
            }
            type_layout::TYPE_OPTION => {
                let inner = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::Option(Box::new(inner))
            }
            type_layout::TYPE_SETTABLE => {
                let inner = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::Settable(Box::new(inner))
            }
            type_layout::TYPE_ARRAY => {
                let inner = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::Array(Box::new(inner))
            }
            "Promise" => {
                let inner = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::Promise(Box::new(inner))
            }
            _ => {
                // Check if this is a known user-defined type or imported name.
                // Skip validation during type registration (forward references).
                // If the env has a Foreign type, preserve it.
                if let Some(Type::Foreign(_)) = self.env.lookup(name) {
                    Type::Foreign(name.to_string())
                } else if self.registering_types
                    || self.env.lookup_type(name).is_some()
                    || self.env.lookup(name).is_some()
                    || name.contains('.')
                {
                    Type::Named(name.to_string())
                } else {
                    self.diagnostics.push(
                        Diagnostic::error(format!("unknown type `{name}`"), span)
                            .with_label("not defined")
                            .with_help("check the spelling or import/define this type")
                            .with_code("E002"),
                    );
                    Type::Unknown
                }
            }
        }
    }

    // ── Item Checking ────────────────────────────────────────────

    fn check_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Import(decl) => self.check_import(decl),
            ItemKind::Const(decl) => self.check_const(decl, item.span),
            ItemKind::Function(decl) => self.check_function(decl, item.span),
            ItemKind::TypeDecl(decl) => self.validate_type_decl_annotations(decl),
            ItemKind::ForBlock(block) => self.check_for_block(block, item.span),
            ItemKind::TraitDecl(decl) => self.check_trait_decl(decl),
            ItemKind::TestBlock(block) => self.check_test_block(block),
            ItemKind::Expr(expr) => {
                let ty = self.check_expr(expr);
                // Rule 5: No floating Results/Options
                if ty.is_result() {
                    self.diagnostics.push(
                        Diagnostic::error("unhandled `Result` value", expr.span)
                            .with_label("this `Result` is not used")
                            .with_help("use `?`, `match`, or assign to `_`")
                            .with_code("E005"),
                    );
                }
            }
        }
    }

    fn check_import(&mut self, decl: &ImportDecl) {
        // Look up resolved symbols for this import source
        let resolved = self.resolved_imports.get(&decl.source).cloned();
        let dts_exports = self.dts_imports.get(&decl.source).cloned();

        for spec in &decl.specifiers {
            let effective_name = spec.alias.as_deref().unwrap_or(&spec.name);

            // Try to find the actual type from resolved imports
            let ty = if let Some(ref resolved) = resolved {
                self.lookup_resolved_symbol(&spec.name, resolved)
            } else if let Some(ref exports) = dts_exports {
                // Look up in .d.ts exports
                exports
                    .iter()
                    .find(|e| e.name == spec.name)
                    .map(|e| interop::wrap_boundary_type(&e.ts_type))
                    .unwrap_or_else(|| Type::Foreign(spec.name.clone()))
            } else {
                // No .fl resolution and no .d.ts — type is foreign to Floe
                Type::Foreign(spec.name.clone())
            };

            self.env.define(effective_name, ty);
            self.unused.defined_sources.insert(
                effective_name.to_string(),
                format!("import from \"{}\"", decl.source),
            );
            self.unused
                .imported_names
                .push((effective_name.to_string(), spec.span));

            // Track untrusted imports (not trusted at module or specifier level).
            // Floe-to-Floe imports (resolved.is_some()) are always trusted.
            if !decl.trusted && !spec.trusted && resolved.is_none() {
                self.untrusted_imports.insert(effective_name.to_string());
            }
        }

        // Auto-import for-blocks when importing a type from the same file
        // (importing a type brings its for-block functions from that file)
        if let Some(ref resolved) = resolved {
            for spec in &decl.specifiers {
                // Check if this specifier is a type in the resolved module
                let is_type = resolved.type_decls.iter().any(|d| d.name == spec.name);
                if is_type {
                    for block in &resolved.for_blocks {
                        let base_type_name = match &block.type_name.kind {
                            TypeExprKind::Named { name, .. } => name.clone(),
                            _ => continue,
                        };
                        if base_type_name == spec.name {
                            self.check_for_block_imported_with_source(block, &decl.source);
                        }
                    }
                }
            }
        }

        // Handle `for Type` import specifiers (cross-file for-blocks)
        if !decl.for_specifiers.is_empty()
            && let Some(ref resolved) = resolved
        {
            for for_spec in &decl.for_specifiers {
                // Find all for-blocks in the resolved module that match this type
                for block in &resolved.for_blocks {
                    let base_type_name = match &block.type_name.kind {
                        TypeExprKind::Named { name, .. } => name.clone(),
                        _ => continue,
                    };
                    if base_type_name == for_spec.type_name {
                        self.check_for_block_imported_with_source(block, &decl.source);
                        // Mark the for-import functions as used (suppress unused import)
                        for func in &block.functions {
                            self.unused.used_names.insert(func.name.clone());
                        }
                    }
                }
            }
        }
    }

    /// Look up a symbol name in resolved imports and return its type.
    fn lookup_resolved_symbol(&mut self, name: &str, resolved: &ResolvedImports) -> Type {
        // Check type declarations
        for decl in &resolved.type_decls {
            if decl.name == name {
                // The type was already registered in the pre-registration pass,
                // so just look it up from the env
                if let Some(ty) = self.env.lookup(name).cloned() {
                    return ty;
                }
                return Type::Named(name.to_string());
            }
        }

        // Check function declarations
        for func in &resolved.function_decls {
            if func.name == name {
                if let Some(ty) = self.env.lookup(name).cloned() {
                    return ty;
                }
                return Type::Unknown;
            }
        }

        // Check const names
        for const_name in &resolved.const_names {
            if const_name == name {
                return Type::Unknown;
            }
        }

        // Not found in resolved module — fall back to Unknown
        Type::Unknown
    }

    /// Register for-block functions from an imported module without checking bodies.
    fn check_for_block_imported_with_source(&mut self, block: &ForBlock, source: &str) {
        let type_name = match &block.type_name.kind {
            TypeExprKind::Named { name, .. } => name.clone(),
            _ => String::new(),
        };

        // Ensure the for-block's target type is defined before resolving.
        // The type may be foreign (from npm/TS, not defined in Floe).
        if !type_name.is_empty() && self.env.lookup(&type_name).is_none() {
            self.env
                .define(&type_name, Type::Foreign(type_name.clone()));
        }

        let for_type = self.resolve_type(&block.type_name);

        for func in &block.functions {
            let return_type = func
                .return_type
                .as_ref()
                .map(|t| self.resolve_type(t))
                .unwrap_or(Type::Unknown);

            let param_types: Vec<_> = func
                .params
                .iter()
                .map(|p| {
                    if p.name == "self" {
                        for_type.clone()
                    } else {
                        p.type_ann
                            .as_ref()
                            .map(|t| self.resolve_type(t))
                            .unwrap_or(Type::Unknown)
                    }
                })
                .collect();

            let fn_type = Type::Function {
                params: param_types,
                return_type: Box::new(return_type),
            };
            self.env.define(&func.name, fn_type.clone());
            self.unused.defined_sources.insert(
                func.name.clone(),
                format!("for-block function from \"{}\"", source),
            );
            self.for_block_overloads
                .entry(func.name.clone())
                .or_default()
                .push((type_name.clone(), fn_type));

            // Track required (non-default) parameter count
            let required_params = func.params.iter().filter(|p| p.default.is_none()).count();
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

    fn check_const(&mut self, decl: &ConstDecl, span: Span) {
        let value_type = self.check_expr(&decl.value);
        let declared_type = decl.type_ann.as_ref().map(|t| self.resolve_type(t));

        // Refine None: if value is Option<Unknown> and declared type is Option<T>,
        // Refine None: record the concrete Option type for hover
        if let Some(ref declared) = declared_type
            && matches!(&value_type, Type::Option(inner) if matches!(**inner, Type::Unknown))
            && matches!(declared, Type::Option(_))
        {
            self.expr_types.insert(decl.value.id, declared.clone());
        }
        let tsgo_type = self.find_and_consume_tsgo_probe(&decl.binding).map(|ty| {
            // The tsgo probe generates the raw call expression without `await`,
            // so if the Floe code has `await`, unwrap Promise from the probe type.
            if Self::expr_has_await(&decl.value)
                && let Type::Promise(inner) = ty
            {
                return *inner;
            }
            ty
        });
        let final_type = self.resolve_const_type(value_type, declared_type, &tsgo_type, span);

        match &decl.binding {
            ConstBinding::Name(name) => {
                self.define_const_binding(name, final_type, decl.exported, span);
            }
            ConstBinding::Array(names) => {
                let corrected_type = self.correct_usestate_option_type(&final_type, &decl.value);
                let effective_type = corrected_type.as_ref().unwrap_or(&final_type);

                for (i, name) in names.iter().enumerate() {
                    let elem_ty = Self::array_element_type(effective_type, i);
                    self.define_const_binding(name, elem_ty, false, span);
                }
            }
            ConstBinding::Tuple(names) => {
                for (i, name) in names.iter().enumerate() {
                    let elem_ty = Self::tuple_element_type(&final_type, i);
                    self.define_const_binding(name, elem_ty, false, span);
                }
            }
            ConstBinding::Object(names) => {
                self.define_object_destructured_bindings(
                    names,
                    &final_type,
                    tsgo_type.is_some(),
                    span,
                );
            }
        }
    }

    /// Find, consume, and return a probe export matching the predicate.
    /// Prefer "inlined" variants when `prefer_inlined` is true.
    fn consume_probe(
        &mut self,
        predicate: impl Fn(&str) -> bool,
        prefer_inlined: bool,
    ) -> Option<Type> {
        let mut found: Option<(String, usize, DtsExport)> = None;
        let mut found_inlined: Option<(String, usize, DtsExport)> = None;
        'outer: for (spec, exports) in &self.dts_imports {
            for (exp_idx, export) in exports.iter().enumerate() {
                let key = (spec.clone(), exp_idx);
                if self.probe_consumed.contains(&key) || !predicate(&export.name) {
                    continue;
                }
                if prefer_inlined && export.name.contains("inlined") {
                    found_inlined = Some((spec.clone(), exp_idx, export.clone()));
                    break 'outer;
                } else if found.is_none() {
                    found = Some((spec.clone(), exp_idx, export.clone()));
                    if !prefer_inlined {
                        break 'outer;
                    }
                }
            }
        }
        let result = if prefer_inlined {
            found_inlined.or(found)
        } else {
            found
        };
        if let Some((spec, exp_idx, _)) = &result {
            self.probe_consumed.insert((spec.clone(), *exp_idx));
        }
        result.map(|(_, _, e)| interop::wrap_boundary_type(&e.ts_type))
    }

    /// Search dts_imports for a tsgo probe matching the binding name, consume it, and return its type.
    fn find_and_consume_tsgo_probe(&mut self, binding: &ConstBinding) -> Option<Type> {
        let binding_name = binding.binding_name();
        let probe_key = format!("__probe_{binding_name}");
        let probe_prefix = format!("__probe_{binding_name}_");
        self.consume_probe(
            |name| name == probe_key || name.starts_with(&probe_prefix),
            true,
        )
    }

    /// Search for a per-field inlined probe (e.g. `__probe_data_inlined_N` for field `data`).
    fn find_per_field_probe(&mut self, field_name: &str) -> Option<Type> {
        let prefix = format!("__probe_{field_name}_inlined_");
        self.consume_probe(|name| name.starts_with(&prefix), false)
    }

    /// Determine the final type for a const binding given value type, declared type, and tsgo probe.
    fn resolve_const_type(
        &mut self,
        value_type: Type,
        declared_type: Option<Type>,
        tsgo_type: &Option<Type>,
        span: Span,
    ) -> Type {
        if let Some(tsgo_ty) = tsgo_type {
            tsgo_ty.clone()
        } else if let Some(ref declared) = declared_type {
            if matches!(value_type, Type::Unknown) && !matches!(declared, Type::Unknown) {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "cannot narrow `unknown` to `{}` — use runtime validation instead",
                            declared.display_name()
                        ),
                        span,
                    )
                    .with_label("unsafe narrowing from `unknown`")
                    .with_help("use a validation library like Zod, or match on the value")
                    .with_code("E019"),
                );
            } else if !self.types_compatible(declared, &value_type) {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "expected `{}`, found `{}`",
                            declared.display_name(),
                            value_type.display_name()
                        ),
                        span,
                    )
                    .with_label(format!("expected `{}`", declared.display_name()))
                    .with_code("E001"),
                );
            }
            declared.clone()
        } else {
            value_type
        }
    }

    /// Check if an expression is wrapped in `await` (possibly through `try`).
    fn expr_has_await(expr: &Expr) -> bool {
        expr_has_await(expr)
    }

    /// Infer the type of an element from an array/tuple destructuring at a given index.
    fn array_element_type(effective_type: &Type, i: usize) -> Type {
        match effective_type {
            Type::Tuple(types) => types.get(i).cloned().unwrap_or(Type::Unknown),
            Type::Unknown | Type::Var(_) => Type::Unknown,
            other if i == 0 => other.clone(),
            _ => Type::Unknown,
        }
    }

    /// Infer the type of a tuple element at a given index.
    fn tuple_element_type(final_type: &Type, i: usize) -> Type {
        match final_type {
            Type::Tuple(types) => types.get(i).cloned().unwrap_or(Type::Unknown),
            Type::Unknown | Type::Var(_) => Type::Unknown,
            _ => Type::Unknown,
        }
    }

    /// Define a single const binding (handles no-redefinition check, name_types, env, etc.)
    fn define_const_binding(&mut self, name: &str, ty: Type, exported: bool, span: Span) {
        self.check_no_redefinition(name, span);
        self.name_types.insert(name.to_string(), ty.display_name());
        self.env.define(name, ty);
        self.unused
            .defined_sources
            .insert(name.to_string(), "const".to_string());
        if exported {
            self.unused.used_names.insert(name.to_string());
        }
        self.unused.defined_names.push((name.to_string(), span));
    }

    /// Handle object destructuring for const bindings.
    fn define_object_destructured_bindings(
        &mut self,
        fields: &[ObjectDestructureField],
        final_type: &Type,
        has_tsgo: bool,
        span: Span,
    ) {
        let concrete = self
            .env
            .resolve_to_concrete(final_type, &expr::simple_resolve_type_expr);

        let field_map: Option<std::collections::HashMap<&str, &Type>> = match &concrete {
            Type::Record(rec_fields) => {
                Some(rec_fields.iter().map(|(n, t)| (n.as_str(), t)).collect())
            }
            _ => None,
        };

        // If tsgo resolved a single-field destructure and the type isn't a Record
        // with that field, assign it directly (legacy behavior for field-level probes)
        if has_tsgo
            && fields.len() == 1
            && field_map
                .as_ref()
                .is_none_or(|m| !m.contains_key(fields[0].field.as_str()))
        {
            self.define_const_binding(fields[0].bound_name(), final_type.clone(), false, span);
            return;
        }

        for f in fields {
            // Look up by the original field name in the source type
            let field_ty = field_map
                .as_ref()
                .and_then(|m| m.get(f.field.as_str()))
                .cloned()
                .cloned();
            // If no field found (e.g. Foreign type), try a per-field probe lookup
            let ty = field_ty
                .unwrap_or_else(|| self.find_per_field_probe(&f.field).unwrap_or(Type::Unknown));
            self.define_const_binding(f.bound_name(), ty, false, span);
        }
    }

    fn check_function(&mut self, decl: &FunctionDecl, span: Span) {
        // Rule: Exported functions must declare return types
        if decl.exported && decl.return_type.is_none() {
            self.diagnostics.push(
                Diagnostic::error(
                    format!(
                        "exported function `{}` must declare a return type",
                        decl.name
                    ),
                    span,
                )
                .with_label("missing return type")
                .with_help("add `-> ReturnType` after the parameter list")
                .with_code("E010"),
            );
        }

        // Register generic type parameters so they're recognized during type resolution
        for tp in &decl.type_params {
            self.env.define(tp, Type::Named(tp.clone()));
        }

        let return_type = decl
            .return_type
            .as_ref()
            .map(|t| self.resolve_type(t))
            .unwrap_or_else(|| self.fresh_type_var());

        // Define function in outer scope before checking body
        let param_types: Vec<_> = decl
            .params
            .iter()
            .map(|p| {
                p.type_ann
                    .as_ref()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or_else(|| self.fresh_type_var())
            })
            .collect();

        let fn_type = Type::Function {
            params: param_types.clone(),
            return_type: Box::new(return_type.clone()),
        };
        self.check_no_redefinition(&decl.name, span);
        self.env.define(&decl.name, fn_type);
        self.unused
            .defined_sources
            .insert(decl.name.clone(), "function".to_string());

        // Track required (non-default) parameter count
        let required_params = decl.params.iter().filter(|p| p.default.is_none()).count();
        if required_params < decl.params.len() {
            self.fn_required_params
                .insert(decl.name.clone(), required_params);
        }

        // Track parameter names for named argument validation
        self.fn_param_names.insert(
            decl.name.clone(),
            decl.params.iter().map(|p| p.name.clone()).collect(),
        );

        if decl.exported {
            self.unused.used_names.insert(decl.name.clone());
        }
        self.unused.defined_names.push((decl.name.clone(), span));

        // Set up scope for function body
        let prev_return_type = self.ctx.current_return_type.take();
        let prev_inside_async = self.ctx.inside_async;
        self.ctx.current_return_type = Some(return_type.clone());
        self.ctx.inside_async = decl.async_fn;

        self.env.push_scope();

        // Define parameters (check for shadowing, but skip `self`)
        for (param, ty) in decl.params.iter().zip(param_types.iter()) {
            if param.name != "self" {
                self.check_no_redefinition(&param.name, span);
            }
            self.env.define(&param.name, ty.clone());

            // For destructured params, define the individual field names in scope
            if let Some(ref destructure) = param.destructure {
                match destructure {
                    ParamDestructure::Object(fields) => {
                        for f in fields {
                            let field_ty = match ty {
                                Type::Record(rec_fields) => rec_fields
                                    .iter()
                                    .find(|(n, _)| n == &f.field)
                                    .map(|(_, t)| t.clone())
                                    .unwrap_or(Type::Unknown),
                                _ => Type::Unknown,
                            };
                            self.env.define(f.bound_name(), field_ty);
                        }
                    }
                    ParamDestructure::Array(fields) => {
                        for field in fields {
                            self.env.define(field, Type::Unknown);
                        }
                    }
                }
            }
        }

        // Type-check default parameter values
        for (param, ty) in decl.params.iter().zip(param_types.iter()) {
            if let Some(default_expr) = &param.default {
                let default_ty = self.check_expr(default_expr);
                if !self.types_compatible(ty, &default_ty) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "default value for `{}`: expected `{}`, found `{}`",
                                param.name,
                                ty.display_name(),
                                default_ty.display_name()
                            ),
                            param.span,
                        )
                        .with_label(format!("expected `{}`", ty.display_name()))
                        .with_code("E001"),
                    );
                }
            }
        }

        // Check body
        let body_type = self.check_expr(&decl.body);

        // When no return type annotation, infer from body and update the function type
        if decl.return_type.is_none() && !matches!(body_type, Type::Var(_) | Type::Unknown) {
            let fn_type = Type::Function {
                params: param_types.clone(),
                return_type: Box::new(body_type.clone()),
            };
            // Update in the name_types map for hover display
            self.name_types
                .insert(decl.name.clone(), fn_type.display_name());
            // Mark for updating in outer scope after pop
            self.env.define_in_parent_scope(&decl.name, fn_type);
        }

        // Check return type compatibility
        if let Some(ref declared_return) = decl.return_type {
            let resolved = self.resolve_type(declared_return);
            // For async functions, unwrap Promise from the declared type since
            // the body type is the unwrapped inner value
            let effective_declared = match (&resolved, decl.async_fn) {
                (Type::Promise(inner), true) => inner.as_ref().clone(),
                _ => resolved.clone(),
            };
            if !self.types_compatible(&effective_declared, &body_type)
                && !matches!(body_type, Type::Var(_))
            {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "function `{}`: expected return type `{}`, found `{}`",
                            decl.name,
                            resolved.display_name(),
                            body_type.display_name()
                        ),
                        span,
                    )
                    .with_label(format!("expected `{}`", resolved.display_name()))
                    .with_code("E001"),
                );
            }

            // Rule: non-unit functions must have an explicit return value
            if !matches!(resolved, Type::Unit)
                && matches!(body_type, Type::Unit)
                && !self.body_has_return(&decl.body)
            {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "function `{}` must return a value of type `{}`",
                            decl.name,
                            resolved.display_name()
                        ),
                        span,
                    )
                    .with_label("missing return value")
                    .with_help("add a return expression or change return type to `()`")
                    .with_code("E013"),
                );
            }
        }

        self.env.pop_scope();
        self.ctx.current_return_type = prev_return_type;
        self.ctx.inside_async = prev_inside_async;
    }

    fn check_for_block(&mut self, block: &ForBlock, _span: Span) {
        let for_type = self.resolve_type(&block.type_name);
        let type_name = match &block.type_name.kind {
            TypeExprKind::Named { name, .. } => name.clone(),
            _ => String::new(),
        };

        // If this is a trait impl block, validate the trait contract
        if let Some(ref trait_name) = block.trait_name {
            self.unused.used_names.insert(trait_name.clone());
            let type_display = for_type.display_name();
            self.check_trait_impl(&type_display, trait_name, &block.functions, block.span);
        }

        for func in &block.functions {
            // Check each function, injecting `self` type for self params
            let return_type = func
                .return_type
                .as_ref()
                .map(|t| self.resolve_type(t))
                .unwrap_or_else(|| self.fresh_type_var());

            let param_types: Vec<_> = func
                .params
                .iter()
                .map(|p| {
                    if p.name == "self" {
                        for_type.clone()
                    } else {
                        p.type_ann
                            .as_ref()
                            .map(|t| self.resolve_type(t))
                            .unwrap_or_else(|| self.fresh_type_var())
                    }
                })
                .collect();

            let fn_type = Type::Function {
                params: param_types.clone(),
                return_type: Box::new(return_type.clone()),
            };
            // Allow for-block functions with the same name on different types
            // (e.g. Entry.fromRow and Accent.fromRow are not in conflict)
            let is_different_for_block = self
                .for_block_overloads
                .get(&func.name)
                .and_then(|o| o.last())
                .is_some_and(|(existing_type, _)| *existing_type != type_name);
            if !is_different_for_block {
                self.check_no_redefinition(&func.name, block.span);
            }
            self.env.define(&func.name, fn_type.clone());
            self.unused
                .defined_sources
                .insert(func.name.clone(), "for-block function".to_string());
            self.for_block_overloads
                .entry(func.name.clone())
                .or_default()
                .push((type_name.clone(), fn_type));

            // Track required (non-default) parameter count
            let required_params = func.params.iter().filter(|p| p.default.is_none()).count();
            if required_params < func.params.len() {
                self.fn_required_params
                    .insert(func.name.clone(), required_params);
            }

            // Track parameter names for named argument validation
            self.fn_param_names.insert(
                func.name.clone(),
                func.params.iter().map(|p| p.name.clone()).collect(),
            );

            if func.exported {
                self.unused.used_names.insert(func.name.clone());
            }
            self.unused
                .defined_names
                .push((func.name.clone(), block.span));

            // Check the function body
            let prev_return_type = self.ctx.current_return_type.take();
            self.ctx.current_return_type = Some(return_type.clone());

            self.env.push_scope();

            for (param, ty) in func.params.iter().zip(param_types.iter()) {
                self.env.define(&param.name, ty.clone());
            }

            // Type-check default parameter values
            for (param, ty) in func.params.iter().zip(param_types.iter()) {
                if let Some(default_expr) = &param.default {
                    let default_ty = self.check_expr(default_expr);
                    if !self.types_compatible(ty, &default_ty) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!(
                                    "default value for `{}`: expected `{}`, found `{}`",
                                    param.name,
                                    ty.display_name(),
                                    default_ty.display_name()
                                ),
                                param.span,
                            )
                            .with_label(format!("expected `{}`", ty.display_name()))
                            .with_code("E001"),
                        );
                    }
                }
            }

            let body_type = self.check_expr(&func.body);

            if let Some(ref declared_return) = func.return_type {
                let resolved = self.resolve_type(declared_return);
                if !self.types_compatible(&resolved, &body_type)
                    && !matches!(body_type, Type::Var(_))
                {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "function `{}`: expected return type `{}`, found `{}`",
                                func.name,
                                resolved.display_name(),
                                body_type.display_name()
                            ),
                            block.span,
                        )
                        .with_label(format!("expected `{}`", resolved.display_name()))
                        .with_code("E001"),
                    );
                }
            }

            self.env.pop_scope();
            self.ctx.current_return_type = prev_return_type;
        }
    }

    fn check_test_block(&mut self, block: &TestBlock) {
        // Type-check test block body in its own scope
        self.env.push_scope();

        for stmt in &block.body {
            match stmt {
                TestStatement::Assert(expr, span) => {
                    let ty = self.check_expr(expr);
                    // Ensure assert expression evaluates to boolean
                    if !matches!(ty, Type::Bool | Type::Unknown | Type::Var(_)) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!(
                                    "assert expression must be boolean, found `{}`",
                                    ty.display_name()
                                ),
                                *span,
                            )
                            .with_label("expected boolean expression")
                            .with_code("E017"),
                        );
                    }
                }
                TestStatement::Expr(expr) => {
                    self.check_expr(expr);
                }
            }
        }

        self.env.pop_scope();
    }

    /// Checks if a function body contains a value-producing expression
    /// (implicit return). With implicit returns, the last expression in
    /// a block is the return value.
    fn body_has_return(&self, body: &Expr) -> bool {
        match &body.kind {
            // `todo` and `unreachable` are never-returning, so they satisfy return requirements
            ExprKind::Todo | ExprKind::Unreachable => true,
            ExprKind::Block(items) => {
                // Check if the last item is an expression (implicit return)
                items.last().is_some_and(|item| {
                    if let ItemKind::Expr(e) = &item.kind {
                        self.body_has_return(e)
                    } else {
                        false
                    }
                })
            }
            ExprKind::Match { arms, .. } => {
                !arms.is_empty() && arms.iter().all(|arm| self.body_has_return(&arm.body))
            }
            // Any other expression is a value-producing expression
            _ => true,
        }
    }

    // ── Trait Declarations ────────────────────────────────────────

    fn register_trait_decl(&mut self, decl: &TraitDecl) {
        let methods: Vec<TraitMethodSig> = decl
            .methods
            .iter()
            .map(|m| TraitMethodSig {
                name: m.name.clone(),
                has_default: m.body.is_some(),
            })
            .collect();
        self.traits.trait_defs.insert(decl.name.clone(), methods);
    }

    fn check_trait_decl(&mut self, decl: &TraitDecl) {
        // Validate method signatures (return types, param types)
        for method in &decl.methods {
            if let Some(ref rt) = method.return_type {
                self.resolve_type(rt);
            }
            for param in &method.params {
                if let Some(ref ta) = param.type_ann {
                    self.resolve_type(ta);
                }
            }
            // Default bodies are NOT type-checked here. They reference other
            // trait methods (like `self |> eq(other)`) which aren't defined yet.
            // The bodies will be checked when used in a concrete for-block impl.
        }

        if decl.exported {
            self.unused.used_names.insert(decl.name.clone());
        }
    }

    /// Validate that a `for Type: Trait` block satisfies the trait contract.
    fn check_trait_impl(
        &mut self,
        type_name: &str,
        trait_name: &str,
        functions: &[FunctionDecl],
        span: Span,
    ) {
        let trait_methods = match self.traits.trait_defs.get(trait_name) {
            Some(methods) => methods.clone(),
            None => {
                self.diagnostics.push(
                    Diagnostic::error(format!("unknown trait `{trait_name}`"), span)
                        .with_label("not defined")
                        .with_help("check the spelling or define this trait")
                        .with_code("E017"),
                );
                return;
            }
        };

        // Check that all required methods are implemented
        let impl_names: HashSet<&str> = functions.iter().map(|f| f.name.as_str()).collect();

        for method in &trait_methods {
            if !method.has_default && !impl_names.contains(method.name.as_str()) {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "trait `{trait_name}` requires method `{}` but it is not implemented for `{type_name}`",
                            method.name
                        ),
                        span,
                    )
                    .with_label(format!("missing method `{}`", method.name))
                    .with_help(format!(
                        "add `fn {}(self, ...) {{ ... }}` to the for block",
                        method.name
                    ))
                    .with_code("E018"),
                );
            }
        }

        // Record the implementation
        self.traits
            .trait_impls
            .insert((type_name.to_string(), trait_name.to_string()));
    }

    // ── JSX Checking ─────────────────────────────────────────────

    fn check_jsx(&mut self, element: &JsxElement) {
        match &element.kind {
            JsxElementKind::Element {
                name,
                props,
                children,
                ..
            } => {
                if name.starts_with(|c: char| c.is_uppercase()) {
                    self.unused.used_names.insert(name.clone());
                    if self.env.lookup(name).is_none() {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!("component `{name}` is not defined"),
                                element.span,
                            )
                            .with_label("not found in scope")
                            .with_code("E002"),
                        );
                    }
                }
                for prop in props {
                    match prop {
                        JsxProp::Named { name, value, .. } => {
                            if let Some(value) = value {
                                // For event handler props, set context so lambda params get event type
                                if name.starts_with("on") && name.len() > 2 {
                                    let prev = self.ctx.event_handler_context;
                                    self.ctx.event_handler_context = true;
                                    self.check_expr(value);
                                    self.ctx.event_handler_context = prev;
                                } else {
                                    self.check_expr(value);
                                }
                            }
                        }
                        JsxProp::Spread { expr, .. } => {
                            self.check_expr(expr);
                        }
                    }
                }
                self.check_jsx_children(children);
            }
            JsxElementKind::Fragment { children } => {
                self.check_jsx_children(children);
            }
        }
    }

    fn check_jsx_children(&mut self, children: &[JsxChild]) {
        for child in children {
            match child {
                JsxChild::Expr(e) => {
                    self.check_expr(e);
                }
                JsxChild::Element(el) => {
                    self.check_jsx(el);
                }
                JsxChild::Text(_) => {}
            }
        }
    }

    // ── Type Compatibility ───────────────────────────────────────

    /// Resolve a `Type::Named` to its concrete underlying type, if possible.
    /// Returns `Some(concrete)` if the type was resolved, `None` if not a Named type.
    fn resolve_named_to_concrete(&self, ty: &Type) -> Option<Type> {
        if let Type::Named(name) = ty {
            let resolved = self
                .env
                .resolve_to_concrete(ty, &expr::simple_resolve_type_expr);
            if &resolved != ty {
                Some(resolved)
            } else {
                self.env.lookup(name).cloned()
            }
        } else {
            None
        }
    }

    /// Like `types_compatible` but treats `unknown` as a wildcard on BOTH sides.
    /// Used for match arm unification where `Result<unknown, E>` should unify
    /// with `Result<T, unknown>`.
    fn types_unifiable(&self, a: &Type, b: &Type) -> bool {
        if matches!(a, Type::Unknown | Type::Var(_)) || matches!(b, Type::Unknown | Type::Var(_)) {
            return true;
        }
        match (a, b) {
            (Type::Result { ok: o1, err: e1 }, Type::Result { ok: o2, err: e2 }) => {
                self.types_unifiable(o1, o2) && self.types_unifiable(e1, e2)
            }
            (Type::Option(a), Type::Option(b)) => self.types_unifiable(a, b),
            (Type::Promise(a), Type::Promise(b)) => self.types_unifiable(a, b),
            (Type::Array(a), Type::Array(b)) => self.types_unifiable(a, b),
            (Type::Tuple(a), Type::Tuple(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| self.types_unifiable(x, y))
            }
            _ => self.types_compatible(a, b),
        }
    }

    /// Merge two types by filling `unknown` holes from the other side.
    /// `Result<unknown, AuthError>` + `Result<Option<Session>, unknown>` →
    /// `Result<Option<Session>, AuthError>`
    fn merge_types(a: &Type, b: &Type) -> Type {
        match (a, b) {
            (Type::Unknown, _) => b.clone(),
            (_, Type::Unknown) => a.clone(),
            (Type::Result { ok: o1, err: e1 }, Type::Result { ok: o2, err: e2 }) => Type::Result {
                ok: Box::new(Self::merge_types(o1, o2)),
                err: Box::new(Self::merge_types(e1, e2)),
            },
            (Type::Option(a_inner), Type::Option(b_inner)) => {
                Type::Option(Box::new(Self::merge_types(a_inner, b_inner)))
            }
            (Type::Promise(a_inner), Type::Promise(b_inner)) => {
                Type::Promise(Box::new(Self::merge_types(a_inner, b_inner)))
            }
            (Type::Array(a_inner), Type::Array(b_inner)) => {
                Type::Array(Box::new(Self::merge_types(a_inner, b_inner)))
            }
            (Type::Tuple(a_elems), Type::Tuple(b_elems)) if a_elems.len() == b_elems.len() => {
                Type::Tuple(
                    a_elems
                        .iter()
                        .zip(b_elems.iter())
                        .map(|(x, y)| Self::merge_types(x, y))
                        .collect(),
                )
            }
            _ => a.clone(),
        }
    }

    fn types_compatible(&self, expected: &Type, actual: &Type) -> bool {
        // Unknown/Var as EXPECTED: anything can be assigned to unknown (widening)
        if matches!(expected, Type::Unknown | Type::Var(_)) {
            return true;
        }
        // Var as ACTUAL: type variables are still being inferred, allow them
        if matches!(actual, Type::Var(_)) {
            return true;
        }
        // Unknown as ACTUAL with concrete expected: NOT compatible.
        // Must narrow unknown before assigning to a concrete type.
        // (This is the key strictness rule — same as TypeScript's unknown.)

        // `never` is compatible with any type (it means "this code never returns")
        if matches!(actual, Type::Never) || matches!(expected, Type::Never) {
            return true;
        }

        // Generic type parameters (single uppercase letter like T, U, E, S)
        // are wildcards that match any type — used in stdlib function signatures
        if let Type::Named(n) = expected
            && n.len() == 1
            && n.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        {
            return true;
        }
        if let Type::Named(n) = actual
            && n.len() == 1
            && n.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        {
            return true;
        }

        // Foreign types (from npm imports) are assumed compatible since we can't
        // fully resolve type aliases across the npm boundary.
        if matches!(expected, Type::Foreign(_)) && !matches!(actual, Type::Unknown) {
            return true;
        }
        if matches!(actual, Type::Foreign(_)) && !matches!(expected, Type::Unknown) {
            return true;
        }

        // Opaque type: within the defining module, the underlying type
        // is assignable to the opaque type (e.g. returning `string` as `HashedPassword`).
        // Currently all code lives in a single file, so same-file = defining module.
        // Supports both `opaque type X { T }` (newtype) and `opaque type X = T` (alias).
        if let Type::Named(name) = expected
            && let Some(info) = self.env.lookup_type(name)
            && info.opaque
        {
            let underlying = match &info.def {
                crate::parser::ast::TypeDef::Alias(type_expr) => {
                    Some(expr::simple_resolve_type_expr(type_expr))
                }
                crate::parser::ast::TypeDef::Union(variants)
                    if variants.len() == 1
                        && variants[0].fields.len() == 1
                        && variants[0].fields[0].name.is_none() =>
                {
                    Some(expr::simple_resolve_type_expr(
                        &variants[0].fields[0].type_ann,
                    ))
                }
                _ => None,
            };
            if let Some(underlying) = underlying
                && self.types_compatible(&underlying, actual)
            {
                return true;
            }
        }

        // Resolve Named types to concrete for structural comparison
        let expected_concrete = self.resolve_named_to_concrete(expected);
        let actual_concrete = self.resolve_named_to_concrete(actual);

        // Named<->Record structural comparison
        if let Some(Type::Record(ref exp_fields)) = expected_concrete
            && let Type::Record(act_fields) = actual
        {
            return exp_fields.len() == act_fields.len()
                && exp_fields.iter().all(|(name, ty)| {
                    act_fields
                        .iter()
                        .any(|(n, t)| n == name && self.types_compatible(ty, t))
                });
        }
        if let Some(Type::Record(ref act_fields)) = actual_concrete
            && let Type::Record(exp_fields) = expected
        {
            return exp_fields.len() == act_fields.len()
                && exp_fields.iter().all(|(name, ty)| {
                    act_fields
                        .iter()
                        .any(|(n, t)| n == name && self.types_compatible(ty, t))
                });
        }

        match (expected, actual) {
            (Type::Number, Type::Number)
            | (Type::String, Type::String)
            | (Type::Bool, Type::Bool)
            | (Type::Unit, Type::Unit)
            | (Type::Undefined, Type::Undefined) => true,
            (Type::Named(a), Type::Named(b)) => a == b,
            (Type::Named(a), Type::Union { name: b, .. })
            | (Type::Union { name: a, .. }, Type::Named(b)) => a == b,
            (Type::Union { name: a, .. }, Type::Union { name: b, .. }) => a == b,
            (Type::Result { ok: o1, err: e1 }, Type::Result { ok: o2, err: e2 }) => {
                self.types_compatible(o1, o2) && self.types_compatible(e1, e2)
            }
            (Type::Option(_), Type::Option(b)) if matches!(**b, Type::Unknown) => {
                true // None (Option<Unknown>) is compatible with any Option<T>
            }
            (Type::Option(a), Type::Option(b)) => self.types_compatible(a, b),
            // A concrete value T is assignable to Option<T> (implicit Some wrapping)
            (Type::Option(inner), actual) => self.types_compatible(inner, actual),
            (Type::Promise(a), Type::Promise(b)) => self.types_compatible(a, b),
            (Type::Settable(_), Type::Settable(b)) if matches!(**b, Type::Unknown) => {
                true // Clear/Unchanged (Settable<Unknown>) is compatible with any Settable<T>
            }
            (Type::Settable(a), Type::Settable(b)) => self.types_compatible(a, b),
            (Type::Array(_), Type::Array(b)) if matches!(**b, Type::Unknown) => {
                true // empty array [] is compatible with any Array<T>
            }
            (Type::Array(a), Type::Array(b)) => self.types_compatible(a, b),
            (
                Type::Map { key: k1, value: v1 } | Type::RecordMap { key: k1, value: v1 },
                Type::Map { key: k2, value: v2 } | Type::RecordMap { key: k2, value: v2 },
            ) => self.types_compatible(k1, k2) && self.types_compatible(v1, v2),
            (Type::Set { element: e1 }, Type::Set { element: e2 }) => self.types_compatible(e1, e2),
            (Type::Tuple(a), Type::Tuple(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| self.types_compatible(x, y))
            }
            (
                Type::Function {
                    params: p1,
                    return_type: r1,
                },
                Type::Function {
                    params: p2,
                    return_type: r2,
                },
            ) => {
                p1.len() == p2.len()
                    && p1
                        .iter()
                        .zip(p2.iter())
                        .all(|(x, y)| self.types_compatible(x, y))
                    && self.types_compatible(r1, r2)
            }
            // Structural record compatibility: { a: T, b: U } matches { a: T, b: U }
            (Type::Record(fields_a), Type::Record(fields_b)) => {
                fields_a.len() == fields_b.len()
                    && fields_a.iter().all(|(name_a, ty_a)| {
                        fields_b.iter().any(|(name_b, ty_b)| {
                            name_a == name_b && self.types_compatible(ty_a, ty_b)
                        })
                    })
            }
            _ => false,
        }
    }
}
