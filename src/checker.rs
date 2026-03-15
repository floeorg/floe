mod expr;
mod match_check;
#[cfg(test)]
mod tests;
mod types;

pub use types::Type;

use std::collections::{HashMap, HashSet};

/// Maps expression spans (start, end) to their resolved types.
pub type ExprTypeMap = HashMap<(usize, usize), Type>;

use crate::diagnostic::Diagnostic;
use crate::interop::{self, DtsExport};
use crate::lexer::span::Span;
use crate::parser::ast::*;
use crate::resolve::ResolvedImports;
use crate::stdlib::StdlibRegistry;
use types::{TypeEnv, TypeInfo};

// ── Checker ──────────────────────────────────────────────────────

/// The Floe type checker.
pub struct Checker {
    env: TypeEnv,
    diagnostics: Vec<Diagnostic>,
    next_var: usize,
    /// The return type of the current function (for ? validation).
    current_return_type: Option<Type>,
    /// Track used variables per scope for unused detection.
    used_names: HashSet<String>,
    /// Track defined names with spans for unused detection.
    defined_names: Vec<(String, Span)>,
    /// Track imported names with spans for unused import detection.
    imported_names: Vec<(String, Span)>,
    /// Standard library function registry.
    stdlib: StdlibRegistry,
    /// Maps expression spans (start, end) to their resolved types.
    /// Used by codegen for type-directed pipe resolution.
    expr_types: ExprTypeMap,
    /// Names of untrusted (external TS) imports that require `try`.
    untrusted_imports: HashSet<String>,
    /// Whether we are currently inside a `try` expression.
    inside_try: bool,
    /// Whether we are in the type registration pass (suppress unknown type errors).
    registering_types: bool,
    /// Pre-resolved imports from other .fl files, keyed by import source string.
    resolved_imports: HashMap<String, ResolvedImports>,
    /// Pre-resolved .d.ts exports for npm imports, keyed by specifier (e.g. "react").
    dts_imports: HashMap<String, Vec<DtsExport>>,
    /// When inside a pipe, holds the type of the piped (left) value.
    /// The Call handler uses this to account for the implicit first argument.
    pipe_input_type: Option<Type>,
    /// Maps variable/function names to their inferred type display names.
    /// Accumulated as names are defined so inner-scope names aren't lost.
    name_types: HashMap<String, String>,
    /// Tracks where each name was defined (e.g., "const", "function", "for-block function from \"./todo\"").
    /// Used to provide context in shadowing error messages.
    defined_sources: HashMap<String, String>,
    /// Registered trait declarations: trait name -> methods.
    trait_defs: HashMap<String, Vec<TraitMethodSig>>,
    /// Tracks which (type, trait) pairs have been implemented.
    trait_impls: HashSet<(String, String)>,
}

/// Signature of a trait method (for checking implementations).
#[derive(Debug, Clone)]
struct TraitMethodSig {
    name: String,
    /// Whether this method has a default implementation.
    has_default: bool,
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

impl Checker {
    pub fn new() -> Self {
        Self {
            env: TypeEnv::new(),
            diagnostics: Vec::new(),
            next_var: 0,
            current_return_type: None,
            used_names: HashSet::new(),
            defined_names: Vec::new(),
            imported_names: Vec::new(),
            stdlib: StdlibRegistry::new(),
            expr_types: HashMap::new(),
            untrusted_imports: HashSet::new(),
            inside_try: false,
            registering_types: false,
            resolved_imports: HashMap::new(),
            dts_imports: HashMap::new(),
            pipe_input_type: None,
            name_types: HashMap::new(),
            defined_sources: HashMap::new(),
            trait_defs: HashMap::new(),
            trait_impls: HashSet::new(),
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

    /// Check a program and return (diagnostics, name_type_map).
    /// The name_type_map maps variable/function names to their inferred type display names.
    pub fn check_with_types(self, program: &Program) -> (Vec<Diagnostic>, HashMap<String, String>) {
        let (diags, name_map, _) = self.check_all(program);
        (diags, name_map)
    }

    /// Internal: run all checks and return all maps.
    fn check_all(
        mut self,
        program: &Program,
    ) -> (Vec<Diagnostic>, HashMap<String, String>, ExprTypeMap) {
        // Pre-register types and functions from resolved imports
        self.registering_types = true;
        for resolved in self.resolved_imports.values().cloned().collect::<Vec<_>>() {
            for decl in &resolved.type_decls {
                self.register_type_decl(decl);
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
                self.defined_sources
                    .insert(func.name.clone(), format!("function from \"{}\"", source));
            }
        }

        // First pass: register all type declarations and traits
        self.registering_types = true;
        for item in &program.items {
            match &item.kind {
                ItemKind::TypeDecl(decl) => {
                    self.register_type_decl(decl);
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
        for (name, span) in &self.imported_names {
            if !self.used_names.contains(name) {
                self.diagnostics.push(
                    Diagnostic::error(format!("unused import `{name}`"), *span)
                        .with_label("imported but never used")
                        .with_help("remove this import or use it in the code")
                        .with_code("E009"),
                );
            }
        }

        // Check for unused variables
        for (name, span) in &self.defined_names {
            if !name.starts_with('_') && !self.used_names.contains(name) {
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
    fn check_no_redefinition(&mut self, name: &str, span: Span) {
        if self.env.is_defined_in_any_scope(name) {
            let msg = if let Some(source) = self.defined_sources.get(name) {
                format!("`{name}` is already defined ({source}) and cannot be shadowed")
            } else {
                format!("`{name}` is already defined and cannot be shadowed")
            };
            self.diagnostics.push(
                Diagnostic::error(msg, span)
                    .with_label("already defined")
                    .with_code("E016"),
            );
        }
    }

    // ── Type Registration ────────────────────────────────────────

    fn register_type_decl(&mut self, decl: &TypeDecl) {
        let info = TypeInfo {
            def: decl.def.clone(),
            opaque: decl.opaque,
            type_params: decl.type_params.clone(),
        };
        self.env.define_type(&decl.name, info);

        // Register the type name in the value namespace too (for constructors)
        match &decl.def {
            TypeDef::Record(_) => {
                self.env.define(&decl.name, Type::Named(decl.name.clone()));
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
                // Register each variant constructor
                for (vname, _) in &var_types {
                    self.env.define(vname, union_type.clone());
                }
            }
            TypeDef::Alias(type_expr) => {
                let ty = self.resolve_type(type_expr);
                self.env.define(&decl.name, ty);
            }
        }
    }

    /// Second-pass validation of type annotations within type declarations.
    /// The first pass (register_type_decl) skips unknown type errors for forward references.
    fn validate_type_decl_annotations(&mut self, decl: &TypeDecl) {
        match &decl.def {
            TypeDef::Record(fields) => {
                for field in fields {
                    self.resolve_type(&field.type_ann);
                }
            }
            TypeDef::Union(variants) => {
                for variant in variants {
                    for field in &variant.fields {
                        self.resolve_type(&field.type_ann);
                    }
                }
            }
            TypeDef::Alias(type_expr) => {
                self.resolve_type(type_expr);
            }
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
        }
    }

    fn resolve_named_type(&mut self, name: &str, type_args: &[TypeExpr], span: Span) -> Type {
        // Mark type names as used (e.g. "JSX" from "JSX.Element", or "User")
        let root = name.split('.').next().unwrap_or(name);
        self.used_names.insert(root.to_string());

        match name {
            "number" => Type::Number,
            "string" => Type::String,
            "boolean" => Type::Bool,
            "()" => Type::Unit,
            "undefined" => Type::Undefined,
            "unknown" => Type::Unknown,
            "Result" => {
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
            "Option" => {
                let inner = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::Option(Box::new(inner))
            }
            "Array" => {
                let inner = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::Array(Box::new(inner))
            }
            "Brand" => {
                let base = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                let tag = type_args
                    .get(1)
                    .and_then(|t| {
                        if let TypeExprKind::Named { name, .. } = &t.kind {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                Type::Brand {
                    base: Box::new(base),
                    tag,
                }
            }
            _ => {
                // Check if this is a known user-defined type or imported name.
                // Skip validation during type registration (forward references).
                if self.registering_types
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
                    .unwrap_or(Type::Unknown)
            } else {
                Type::Unknown
            };

            self.env.define(effective_name, ty);
            self.defined_sources.insert(
                effective_name.to_string(),
                format!("import from \"{}\"", decl.source),
            );
            self.imported_names
                .push((effective_name.to_string(), spec.span));

            // Track untrusted imports (not trusted at module or specifier level)
            if !decl.trusted && !spec.trusted {
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
                            self.used_names.insert(func.name.clone());
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
            self.env.define(&func.name, fn_type);
            self.defined_sources.insert(
                func.name.clone(),
                format!("for-block function from \"{}\"", source),
            );
        }
    }

    fn check_const(&mut self, decl: &ConstDecl, span: Span) {
        let value_type = self.check_expr(&decl.value);

        let declared_type = decl.type_ann.as_ref().map(|t| self.resolve_type(t));

        // Check if tsgo resolved a more precise type for this const
        let tsgo_type = {
            let binding_name = match &decl.binding {
                ConstBinding::Name(n) => n.clone(),
                ConstBinding::Array(names) => names.join("_"),
                ConstBinding::Object(names) => names.join("_"),
                ConstBinding::Tuple(names) => names.join("_"),
            };
            let probe_key = format!("__probe_{binding_name}");
            self.dts_imports
                .values()
                .flatten()
                .find(|e| e.name == probe_key)
                .map(|e| interop::wrap_boundary_type(&e.ts_type))
        };

        let final_type = if let Some(tsgo_ty) = tsgo_type {
            // tsgo gave us a fully-resolved type — use it
            tsgo_ty
        } else if let Some(ref declared) = declared_type {
            if !self.types_compatible(declared, &value_type) {
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
        };

        match &decl.binding {
            ConstBinding::Name(name) => {
                self.check_no_redefinition(name, span);
                self.name_types
                    .insert(name.clone(), final_type.display_name());
                self.env.define(name, final_type);
                self.defined_sources
                    .insert(name.clone(), "const".to_string());
                if decl.exported {
                    self.used_names.insert(name.clone());
                }
                self.defined_names.push((name.clone(), span));
            }
            ConstBinding::Array(names) => {
                // Infer element types from the value type
                for (i, name) in names.iter().enumerate() {
                    let elem_ty = match &final_type {
                        // Tuple destructuring: each name gets its positional type
                        Type::Tuple(types) => types.get(i).cloned().unwrap_or(Type::Unknown),
                        // Unknown: no info
                        Type::Unknown | Type::Var(_) => Type::Unknown,
                        // Known type (e.g., Array<Todo> from useState<Array<Todo>>):
                        // first element gets the type, rest get Unknown
                        other if i == 0 => other.clone(),
                        _ => Type::Unknown,
                    };
                    self.check_no_redefinition(name, span);
                    self.name_types.insert(name.clone(), elem_ty.display_name());
                    self.env.define(name, elem_ty);
                    self.defined_sources
                        .insert(name.clone(), "const".to_string());
                    self.defined_names.push((name.clone(), span));
                }
            }
            ConstBinding::Tuple(names) => {
                // Infer element types from the value type
                for (i, name) in names.iter().enumerate() {
                    let elem_ty = match &final_type {
                        Type::Tuple(types) => types.get(i).cloned().unwrap_or(Type::Unknown),
                        Type::Unknown | Type::Var(_) => Type::Unknown,
                        _ => Type::Unknown,
                    };
                    self.check_no_redefinition(name, span);
                    self.name_types.insert(name.clone(), elem_ty.display_name());
                    self.env.define(name, elem_ty);
                    self.defined_sources
                        .insert(name.clone(), "const".to_string());
                    self.defined_names.push((name.clone(), span));
                }
            }
            ConstBinding::Object(names) => {
                // Resolve the value type to find field types for destructuring
                let concrete = {
                    let resolve_fn = |type_expr: &crate::parser::ast::TypeExpr| -> Type {
                        match &type_expr.kind {
                            crate::parser::ast::TypeExprKind::Named { name, .. } => {
                                match name.as_str() {
                                    "number" => Type::Number,
                                    "string" => Type::String,
                                    "boolean" => Type::Bool,
                                    "()" => Type::Unit,
                                    "undefined" => Type::Undefined,
                                    _ => Type::Named(name.to_string()),
                                }
                            }
                            crate::parser::ast::TypeExprKind::Array(inner) => {
                                let inner_resolved = match &inner.kind {
                                    crate::parser::ast::TypeExprKind::Named { name, .. } => {
                                        match name.as_str() {
                                            "number" => Type::Number,
                                            "string" => Type::String,
                                            "boolean" => Type::Bool,
                                            _ => Type::Named(name.to_string()),
                                        }
                                    }
                                    _ => Type::Unknown,
                                };
                                Type::Array(Box::new(inner_resolved))
                            }
                            _ => Type::Unknown,
                        }
                    };
                    self.env.resolve_to_concrete(&final_type, &resolve_fn)
                };

                let field_map: Option<std::collections::HashMap<&str, &Type>> = match &concrete {
                    Type::Record(fields) => {
                        Some(fields.iter().map(|(n, t)| (n.as_str(), t)).collect())
                    }
                    _ => None,
                };

                for name in names {
                    let field_ty = field_map
                        .as_ref()
                        .and_then(|m| m.get(name.as_str()))
                        .cloned()
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    self.check_no_redefinition(name, span);
                    self.name_types
                        .insert(name.clone(), field_ty.display_name());
                    self.env.define(name, field_ty);
                    self.defined_sources
                        .insert(name.clone(), "const".to_string());
                    self.defined_names.push((name.clone(), span));
                }
            }
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
        self.defined_sources
            .insert(decl.name.clone(), "function".to_string());
        if decl.exported {
            self.used_names.insert(decl.name.clone());
        }
        self.defined_names.push((decl.name.clone(), span));

        // Set up scope for function body
        let prev_return_type = self.current_return_type.take();
        self.current_return_type = Some(return_type.clone());

        self.env.push_scope();

        // Define parameters (check for shadowing, but skip `self`)
        for (param, ty) in decl.params.iter().zip(param_types.iter()) {
            if param.name != "self" {
                self.check_no_redefinition(&param.name, span);
            }
            self.env.define(&param.name, ty.clone());
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
            if !self.types_compatible(&resolved, &body_type) && !matches!(body_type, Type::Var(_)) {
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
        self.current_return_type = prev_return_type;
    }

    fn check_for_block(&mut self, block: &ForBlock, _span: Span) {
        let for_type = self.resolve_type(&block.type_name);

        // If this is a trait impl block, validate the trait contract
        if let Some(ref trait_name) = block.trait_name {
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
            self.check_no_redefinition(&func.name, block.span);
            self.env.define(&func.name, fn_type);
            self.defined_sources
                .insert(func.name.clone(), "for-block function".to_string());
            if func.exported {
                self.used_names.insert(func.name.clone());
            }
            self.defined_names.push((func.name.clone(), block.span));

            // Check the function body
            let prev_return_type = self.current_return_type.take();
            self.current_return_type = Some(return_type.clone());

            self.env.push_scope();

            for (param, ty) in func.params.iter().zip(param_types.iter()) {
                self.env.define(&param.name, ty.clone());
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
            self.current_return_type = prev_return_type;
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

    /// Checks if a function body contains a return expression.
    fn body_has_return(&self, body: &Expr) -> bool {
        match &body.kind {
            ExprKind::Return(Some(_)) => true,
            // `todo` and `unreachable` are never-returning, so they satisfy return requirements
            ExprKind::Todo | ExprKind::Unreachable => true,
            ExprKind::Block(items) => items.iter().any(|item| {
                if let ItemKind::Expr(e) = &item.kind {
                    self.body_has_return(e)
                } else {
                    false
                }
            }),
            ExprKind::Match { arms, .. } => {
                !arms.is_empty() && arms.iter().all(|arm| self.body_has_return(&arm.body))
            }
            _ => false,
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
        self.trait_defs.insert(decl.name.clone(), methods);
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
            self.used_names.insert(decl.name.clone());
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
        let trait_methods = match self.trait_defs.get(trait_name) {
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
        self.trait_impls
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
                    self.used_names.insert(name.clone());
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
                    if let Some(ref value) = prop.value {
                        self.check_expr(value);
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

    fn types_compatible(&self, expected: &Type, actual: &Type) -> bool {
        if matches!(expected, Type::Unknown | Type::Var(_))
            || matches!(actual, Type::Unknown | Type::Var(_))
        {
            return true;
        }

        // `never` is compatible with any type (it means "this code never returns")
        if matches!(actual, Type::Never) || matches!(expected, Type::Never) {
            return true;
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
            (Type::Brand { tag: a, .. }, Type::Brand { tag: b, .. }) => a == b,
            (Type::Result { ok: o1, err: e1 }, Type::Result { ok: o2, err: e2 }) => {
                self.types_compatible(o1, o2) && self.types_compatible(e1, e2)
            }
            (Type::Option(a), Type::Option(b)) => self.types_compatible(a, b),
            (Type::Array(a), Type::Array(b)) => self.types_compatible(a, b),
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
            _ => false,
        }
    }
}
