mod attach;
mod environment;
pub mod error_codes;
mod expr;
mod hydrator;
mod imports;
mod items;
mod match_check;
pub mod prelude;
mod printer;
pub mod problems;
#[cfg(test)]
mod tests;
mod traits;
mod type_compat;
mod type_registration;
mod type_resolve;
mod type_var;
mod types;
mod unify;

pub use attach::{
    attach_trait_decl_shallow, attach_type_decl_shallow, attach_types, lower_to_typed,
};
pub use error_codes::ErrorCode;
pub use expr::simple_resolve_type_expr;
pub use prelude::UNKNOWN;
pub use printer::TypeDisplay;
pub use problems::Problems;
pub use type_var::any_nested as type_any_nested;
pub use types::Type;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::parser::ast::ExprId;

/// Maps expression IDs to their resolved types. Stored as `Arc<Type>` so
/// downstream consumers (`attach_types`, LSP hover) can share the same
/// type instance without deep-cloning the Type tree on every lookup.
pub type ExprTypeMap = HashMap<ExprId, Arc<Type>>;

/// Walk the AST and set `async_fn = true` on functions/arrows whose bodies
/// contain `Promise.await` calls or whose return type is `Promise<T>`.
/// Preserves the parser-set `async_fn` flag (from the `async fn` sugar).
/// Also collects untrusted import names from the program for detection.
pub fn mark_async_functions<T>(program: &mut Program<T>) {
    for item in &mut program.items {
        match &mut item.kind {
            ItemKind::Function(decl) => {
                decl.async_fn = decl.async_fn || body_has_promise_await(&decl.body);
            }
            ItemKind::ForBlock(block) => {
                for func in &mut block.functions {
                    func.async_fn = func.async_fn || body_has_promise_await(&func.body);
                }
            }
            _ => {}
        }
    }
    // Mark nested fn declarations and arrows inside all expressions.
    // The walker visits Block/Collect items recursively, so this catches
    // nested `fn` declarations (e.g. handleDragEnd inside a component body).
    crate::walk::walk_program_mut(program, &mut |expr| match &mut expr.kind {
        ExprKind::Arrow { async_fn, body, .. } => {
            *async_fn = *async_fn || body_has_promise_await(body);
        }
        ExprKind::Block(items) | ExprKind::Collect(items) => {
            for item in items {
                if let ItemKind::Function(decl) = &mut item.kind {
                    decl.async_fn = decl.async_fn || body_has_promise_await(&decl.body);
                }
            }
        }
        _ => {}
    });
}

/// Check if an expression body contains a `Promise.await` member access
/// or bare `await` identifier (shorthand for `Promise.await` in pipes).
pub(crate) fn body_has_promise_await<T>(expr: &Expr<T>) -> bool {
    fn walk<T>(expr: &Expr<T>) -> bool {
        match &expr.kind {
            // Qualified: `Promise.await`
            ExprKind::Member { object, field }
                if field == "await"
                    && matches!(&object.kind, ExprKind::Identifier(m) if m == "Promise") =>
            {
                true
            }
            // Bare shorthand: `|> await`
            ExprKind::Identifier(name) if name == "await" => true,
            ExprKind::Call { callee, args, .. } => {
                walk(callee)
                    || args.iter().any(|a| match a {
                        Arg::Positional(e) | Arg::Named { value: e, .. } => walk(e),
                    })
            }
            ExprKind::Pipe { left, right } => walk(left) || walk(right),
            ExprKind::Binary { left, right, .. } => walk(left) || walk(right),
            ExprKind::Member { object, .. } => walk(object),
            ExprKind::Unary { operand, .. }
            | ExprKind::Grouped(operand)
            | ExprKind::Unwrap(operand)
            | ExprKind::Spread(operand)
            | ExprKind::Value(operand) => walk(operand),
            ExprKind::Block(items) | ExprKind::Collect(items) => items.iter().any(|item| {
                match &item.kind {
                    ItemKind::Expr(e) => walk(e),
                    ItemKind::Const(c) => walk(&c.value),
                    ItemKind::Function(_) => false, // don't descend into nested functions
                    _ => false,
                }
            }),
            ExprKind::Match { subject, arms } => {
                walk(subject) || arms.iter().any(|a| walk(&a.body))
            }
            ExprKind::Array(items) | ExprKind::Tuple(items) => items.iter().any(walk),
            // Don't recurse into nested arrows — they're separate async contexts
            ExprKind::Arrow { .. } => false,
            _ => false,
        }
    }
    walk(expr)
}

use crate::diagnostic::Diagnostic;
use crate::interop::{self, DtsExport};
use crate::lexer::span::Span;
use crate::parser::ast::*;
use crate::resolve::ResolvedImports;
use crate::stdlib::StdlibRegistry;
use crate::type_layout;
use environment::{TypeEnv, TypeInfo};

// ── Context flags ────────────────────────────────────────────────

/// Transient context flags that change during recursive expression checking.
/// Bundled together so they can be saved/restored as a unit via `with_context`.
#[derive(Clone, Default)]
pub(crate) struct CheckContext {
    /// The return type of the current function (for ? validation).
    pub current_return_type: Option<Type>,
    /// Whether we are currently inside a `collect` block.
    pub inside_collect: bool,
    /// The error type collected from `?` operations inside a `collect` block.
    pub collect_err_type: Option<Type>,
    /// Whether we are checking an event handler prop value (onChange, onClick, etc.)
    pub event_handler_context: bool,
    /// Hints for lambda parameter type inference from calling context.
    /// Each element corresponds to a parameter position (index 0 = first param, etc.).
    pub lambda_param_hints: Vec<Type>,
    /// When inside a pipe, holds the type of the piped (left) value.
    pub pipe_input_type: Option<Type>,
    /// Expected type from surrounding context (const annotation, function return type, etc.).
    /// Used by Ok/Err for bidirectional inference to fill missing type parameters.
    pub expected_type: Option<Type>,
    /// Name of the function currently being checked (top-level function
    /// declarations only — lambdas and nested scopes leave this as `None`).
    /// Used to look up per-function chain probes so `c.req.param(...)`
    /// inside a handler registered at a specific route can narrow via
    /// the path-threaded Context emitted by the probe generator.
    pub current_function: Option<String>,
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

// ── Checker ──────────────────────────────────────────────────────

/// The Floe type checker.
pub struct Checker {
    env: TypeEnv,
    problems: Problems,
    /// Expressions that produced a type error — `attach_types` converts
    /// them to `ExprKind::Invalid` so codegen skips broken subtrees.
    invalid_exprs: HashSet<ExprId>,
    next_var: u64,
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
    /// Names of untrusted (external TS) imports — npm imports not marked `trusted`.
    untrusted_imports: HashSet<String>,
    /// Names from npm imports (both trusted and untrusted).
    npm_imports: HashSet<String>,
    /// Whether we are in the type registration pass (suppress unknown type errors).
    registering_types: bool,
    /// Pre-resolved imports from other .fl files, keyed by import source string.
    resolved_imports: HashMap<String, ResolvedImports>,
    /// Pre-resolved .d.ts exports for npm imports, keyed by specifier (e.g. "react").
    dts_imports: HashMap<String, Vec<DtsExport>>,
    /// Generic type parameter metadata (name + optional default) captured
    /// from .d.ts declarations, keyed by the generic's simple name
    /// (e.g. "Context"). Used to pad partial type argument lists with
    /// TypeScript's own defaults so a 1-arg user reference can unify with
    /// a 3-arg library reference when the declaration has defaults.
    dts_generic_params: HashMap<String, Vec<crate::interop::GenericParamInfo>>,
    /// Tracks consumed probe exports to prevent reuse.
    /// Uses (specifier_name, export_index) pairs for stable identification
    /// across HashMap iteration orders.
    probe_consumed: HashSet<(String, usize)>,
    /// Maps variable/function names to their inferred type display names.
    /// Accumulated as names are defined so inner-scope names aren't lost.
    name_types: HashMap<String, String>,
    /// Top-level names keyed to their resolved `Type` (in `Arc` so
    /// callers can share without deep-cloning). Populated from the env
    /// at the end of `check_all`; consumed by the LSP for typed pipe
    /// compat. Narrower than `name_types`: only global-scope entries
    /// land here, not inner-scope shadows.
    name_type_map: HashMap<String, std::sync::Arc<Type>>,
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
    /// Ambient type definitions from TypeScript lib files (e.g., lib.dom.d.ts).
    /// Maps interface names (Window, Navigator, Console, etc.) to their Record types.
    /// Used by `resolve_type_to_concrete()` to resolve member access on globals.
    ambient_types: HashMap<String, Type>,
    /// Import sources that resolve to `.ts`/`.tsx` files but could not be
    /// resolved because tsgo is not installed.
    ts_imports_missing_tsgo: HashSet<String>,
    /// User-written generic type parameter names mapped to the `Generic`
    /// variables minted for them. Populated at the top of each fn decl
    /// (see `hydrator::Hydrator`) and cleared when the fn scope pops.
    active_type_params: HashMap<String, Type>,
    /// Tracks `(definition_span, reference_span)` pairs across the whole
    /// program. LSP features (go-to-definition, find-references, rename)
    /// query this side-table instead of re-walking the AST.
    pub(crate) references: crate::reference::ReferenceTracker,
    /// ExprIds of `ExprKind::Todo`/`Unreachable`/`Clear`/`Unchanged` expressions
    /// that turned out to be references to a local binding with the same name
    /// (issue #1226). `attach_types` rewrites these to `ExprKind::Identifier`
    /// so codegen emits a variable read rather than the keyword's runtime
    /// panic / Settable sentinel.
    pub(crate) shadowed_keyword_exprs: HashMap<ExprId, &'static str>,
}

/// Signature of a trait method (for checking implementations).
#[derive(Debug, Clone)]
pub(crate) struct TraitMethodSig {
    pub name: String,
    /// Whether this method has a default implementation.
    pub has_default: bool,
    /// Whether the trait method's first parameter is `self`.
    pub has_self: bool,
    /// Non-self parameters from the trait definition (for signature checking).
    pub params: Vec<Param>,
    /// Return type from the trait definition (for signature checking).
    pub return_type: Option<TypeExpr>,
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
                    return_type: Arc::new(Type::Unknown),
                    required_params: 0,
                },
            ),
            (
                "text".to_string(),
                Type::Function {
                    params: vec![],
                    return_type: Arc::new(Type::String),
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
                    return_type: Arc::new(Type::Unit),
                    required_params: 0,
                },
            ),
            (
                "stopPropagation".to_string(),
                Type::Function {
                    params: vec![],
                    return_type: Arc::new(Type::Unit),
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
                    return_type: Arc::new(Type::Promise(Arc::new(Type::Named(
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
                            return_type: Arc::new(Type::Unit),
                            required_params: 0,
                        },
                        Type::Number,
                    ],
                    return_type: Arc::new(Type::Number),
                    required_params: 2,
                },
            ),
            (
                "setInterval",
                Type::Function {
                    params: vec![
                        Type::Function {
                            params: vec![],
                            return_type: Arc::new(Type::Unit),
                            required_params: 0,
                        },
                        Type::Number,
                    ],
                    return_type: Arc::new(Type::Number),
                    required_params: 2,
                },
            ),
            (
                "clearTimeout",
                Type::Function {
                    params: vec![Type::Number],
                    return_type: Arc::new(Type::Unit),
                    required_params: 1,
                },
            ),
            (
                "clearInterval",
                Type::Function {
                    params: vec![Type::Number],
                    return_type: Arc::new(Type::Unit),
                    required_params: 1,
                },
            ),
            ("Promise", Type::Unknown),
            ("JSON", Type::Unknown),
        ];

        for (name, ty) in browser_globals {
            env.define(name, ty.clone());
        }

        // Browser globals that can throw (untrusted by default)
        let mut untrusted_globals = HashSet::new();
        untrusted_globals.insert("fetch".to_string());

        Self {
            env,
            problems: Problems::new(),
            invalid_exprs: HashSet::new(),
            next_var: 0,
            stdlib: StdlibRegistry::new(),
            expr_types: HashMap::new(),
            ctx: CheckContext::default(),
            unused: UnusedTracker::default(),
            traits: TraitRegistry::default(),
            untrusted_imports: untrusted_globals,
            npm_imports: HashSet::new(),
            registering_types: false,
            resolved_imports: HashMap::new(),
            dts_imports: HashMap::new(),
            dts_generic_params: HashMap::new(),
            probe_consumed: HashSet::new(),
            name_types: HashMap::new(),
            name_type_map: HashMap::new(),
            ambiguous_variants: HashMap::new(),
            fn_required_params: HashMap::new(),
            fn_param_names: HashMap::new(),
            for_block_overloads: HashMap::new(),
            jsx_callback_hints: HashMap::new(),
            jsx_children_hints: HashMap::new(),
            ambient_types: HashMap::new(),
            ts_imports_missing_tsgo: HashSet::new(),
            active_type_params: HashMap::new(),
            references: crate::reference::ReferenceTracker::new(),
            shadowed_keyword_exprs: HashMap::new(),
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

    /// Register generic type parameter metadata (including defaults) from
    /// .d.ts declarations. Each entry maps a generic's simple name to its
    /// positional parameter list. Called by `from_context` with data
    /// parsed from imported `.d.ts` files; exposed publicly so tests can
    /// exercise the default-padding logic with inline fixtures.
    pub fn set_dts_generic_params(
        &mut self,
        params: HashMap<String, Vec<crate::interop::GenericParamInfo>>,
    ) {
        self.dts_generic_params = params;
    }

    /// Create a checker with all available type context.
    ///
    /// This is the single entry point for constructing a checker with imports
    /// and ambient types. Replaces the need to branch between `new()`,
    /// `with_imports()`, `with_all_imports()` at call sites.
    pub fn from_context(
        fl_imports: HashMap<String, ResolvedImports>,
        dts_imports: HashMap<String, Vec<DtsExport>>,
        ambient: Option<crate::interop::ambient::AmbientDeclarations>,
        ts_imports_missing_tsgo: HashSet<String>,
    ) -> Self {
        let mut checker = Self::new();
        checker.resolved_imports = fl_imports;
        checker.dts_imports = dts_imports;
        checker.ts_imports_missing_tsgo = ts_imports_missing_tsgo;
        if let Some(ambient) = ambient {
            checker.register_ambient_types(ambient);
        }
        checker
    }

    /// Register ambient types from TypeScript lib definitions, replacing
    /// the hardcoded browser globals with real typed declarations.
    fn register_ambient_types(&mut self, ambient: crate::interop::ambient::AmbientDeclarations) {
        self.ambient_types = ambient.types;

        // Preserve Floe-specific types (Response, Error, Event) from stdlib
        const PRESERVED: &[&str] = &["Response", "Error", "Event"];

        for (name, ty) in ambient.globals {
            if PRESERVED.contains(&name.as_str()) {
                continue;
            }
            // Skip constructor entries (`declare var Foo: { prototype: Foo }`)
            // that shadow interface definitions — we want the interface for
            // member access, not the constructor object.
            if self.ambient_types.contains_key(&name) && matches!(ty, Type::Record(_)) {
                continue;
            }
            if name == "fetch" {
                self.untrusted_imports.insert(name.clone());
            }
            self.env.define(&name, ty);
        }
    }

    /// Run `f` with modified context flags, then restore the previous context.
    /// This replaces manual save/restore patterns like:
    /// ```ignore
    /// let prev = self.ctx.inside_collect;
    /// self.ctx.inside_collect = true;
    /// // ... work ...
    /// self.ctx.inside_collect = prev;
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

    /// True if `name` was brought in by any `import` declaration — Floe, npm,
    /// or relative TS. Used when `resolve_named_type` needs to distinguish a
    /// locally-declared Floe function (not a type) from an imported alias
    /// like hono's `Next`, which bind identically in the value namespace but
    /// only the latter should be usable in type position.
    pub(crate) fn is_imported_name(&self, name: &str) -> bool {
        self.unused.imported_names.iter().any(|(n, _)| n == name)
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

    /// Check a program and return (diagnostics, expr_type_map, invalid_exprs). The
    /// map keys each expression by its `ExprId` and is consumed by
    /// `attach_types` to produce a `TypedProgram` for codegen. Expressions in
    /// `invalid_exprs` become `ExprKind::Invalid` nodes in the typed tree.
    pub fn check_full(
        mut self,
        program: &Program,
    ) -> (
        Vec<Diagnostic>,
        ExprTypeMap,
        HashSet<ExprId>,
        HashMap<ExprId, &'static str>,
    ) {
        let (diags, _, expr_types, invalid) = self.check_all(program);
        let shadowed = self.take_shadowed_keyword_exprs();
        (diags, expr_types, invalid, shadowed)
    }

    /// Check a program and return diagnostics, name_type_map, expr_type_map,
    /// and invalid_exprs set.
    /// The name_type_map maps variable/function names to their inferred type display names.
    /// The expr_type_map maps `ExprId` to `Arc<Type>` and is consumed by `attach_types`.
    pub fn check_with_types(
        mut self,
        program: &Program,
    ) -> (
        Vec<Diagnostic>,
        HashMap<String, String>,
        ExprTypeMap,
        HashSet<ExprId>,
    ) {
        self.check_all(program)
    }

    /// Check a program and return diagnostics along with the reference
    /// tracker. LSP features consume the tracker to answer go-to-definition
    /// and find-references queries without re-walking the AST.
    pub fn check_with_references(
        mut self,
        program: &Program,
    ) -> (Vec<Diagnostic>, crate::reference::ReferenceTracker) {
        let (diags, _, _, _) = self.check_all(program);
        (diags, self.references)
    }

    /// Take the checker's top-level `name -> Arc<Type>` map.
    pub fn take_name_type_map(&mut self) -> HashMap<String, std::sync::Arc<Type>> {
        std::mem::take(&mut self.name_type_map)
    }

    /// Take the set of keyword-expression IDs (`todo`, `unreachable`, `clear`,
    /// `unchanged`) that turned out to be references to a local binding with
    /// the same name. `attach_types` consumes this to rewrite them into
    /// plain identifier reads (issue #1226).
    pub fn take_shadowed_keyword_exprs(&mut self) -> HashMap<ExprId, &'static str> {
        std::mem::take(&mut self.shadowed_keyword_exprs)
    }

    /// Run all checks and return all maps. Takes `&mut self` so callers
    /// that need additional state off the checker (references, traits,
    /// etc.) can read it afterward.
    #[allow(clippy::type_complexity)]
    pub(crate) fn check_all(
        &mut self,
        program: &Program,
    ) -> (
        Vec<Diagnostic>,
        HashMap<String, String>,
        ExprTypeMap,
        HashSet<ExprId>,
    ) {
        // Pre-register types, traits, and functions from resolved imports
        self.registering_types = true;
        // Register foreign (npm) type names first so they're in scope when
        // resolving fields of imported type declarations
        for resolved in self.resolved_imports.values() {
            for name in &resolved.foreign_type_names {
                self.env.define(name, Type::foreign(name.clone()));
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
                    return_type: Arc::new(return_type),
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

        // Generate __field_ entries for DTS Object types (npm interfaces/types)
        // so dot-completions work for Foreign types like DraggableProvided.
        for exports in self.dts_imports.values() {
            for export in exports {
                if let interop::TsType::Object(fields) = &export.ts_type {
                    for field in fields {
                        let field_ty = interop::wrap_boundary_type(&field.ty);
                        self.name_types.insert(
                            format!("__field_{}_{}", export.name, field.name),
                            field_ty.to_string(),
                        );
                    }
                }
            }
        }

        // Second pass: check all items
        for item in &program.items {
            self.check_item(item);
        }

        self.check_default_exports(program);

        // Check for unused imports
        for (name, span) in &self.unused.imported_names {
            if !self.unused.used_names.contains(name) {
                self.problems.push(
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
                self.problems.push(
                    Diagnostic::warning(format!("unused variable `{name}`"), *span)
                        .with_label("defined but never used")
                        .with_help(format!("prefix with underscore `_{name}` to suppress"))
                        .with_code("W001"),
                );
            }
        }

        // Merge remaining scope entries so top-level names land in both
        // the display map and the typed map.
        for scope in &self.env.scopes {
            for (name, ty) in scope {
                self.name_types
                    .entry(name.clone())
                    .or_insert_with(|| ty.to_string());
                self.name_type_map
                    .entry(name.clone())
                    .or_insert_with(|| std::sync::Arc::new(ty.clone()));
            }
        }

        self.problems.sort();
        (
            self.problems.take(),
            std::mem::take(&mut self.name_types),
            std::mem::take(&mut self.expr_types),
            std::mem::take(&mut self.invalid_exprs),
        )
    }

    // ── Diagnostic helpers ────────────────────────────────────────

    fn emit_error(
        &mut self,
        msg: impl Into<String>,
        span: Span,
        code: ErrorCode,
        label: impl Into<String>,
    ) {
        self.problems.push(
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
        self.problems.push(
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
        self.problems.push(
            Diagnostic::warning(msg, span)
                .with_label(label)
                .with_help(help)
                .with_error_code(code),
        );
    }

    /// Returns true if an error diagnostic has already been emitted within the given span.
    fn has_error_within_span(&self, span: Span) -> bool {
        self.problems.has_error_within_span(span)
    }

    // ── Type helpers ────────────────────────────────────────────────

    fn fresh_type_var(&mut self) -> Type {
        let id = self.next_var;
        self.next_var += 1;
        Type::unbound(id)
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
