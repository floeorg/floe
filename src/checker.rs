mod expr;
mod match_check;
#[cfg(test)]
mod tests;
mod types;

pub use types::Type;

use std::collections::{HashMap, HashSet};

use crate::diagnostic::Diagnostic;
use crate::lexer::span::Span;
use crate::parser::ast::*;
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
    /// Names of untrusted (external TS) imports that require `try`.
    untrusted_imports: HashSet<String>,
    /// Whether we are currently inside a `try` expression.
    inside_try: bool,
    /// Whether we are in the type registration pass (suppress unknown type errors).
    registering_types: bool,
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
            untrusted_imports: HashSet::new(),
            inside_try: false,
            registering_types: false,
        }
    }

    /// Check a program and return diagnostics.
    pub fn check(self, program: &Program) -> Vec<Diagnostic> {
        self.check_with_types(program).0
    }

    /// Check a program and return (diagnostics, type_map).
    /// The type_map maps variable/function names to their inferred type display names.
    pub fn check_with_types(
        mut self,
        program: &Program,
    ) -> (Vec<Diagnostic>, HashMap<String, String>) {
        // First pass: register all type declarations
        self.registering_types = true;
        for item in &program.items {
            if let ItemKind::TypeDecl(decl) = &item.kind {
                self.register_type_decl(decl);
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
                    Diagnostic::error(format!("`{name}` is never used"), *span)
                        .with_label("unused import")
                        .with_help("Remove this import or use it in the code")
                        .with_code("E009"),
                );
            }
        }

        // Check for unused variables
        for (name, span) in &self.defined_names {
            if !name.starts_with('_') && !self.used_names.contains(name) {
                self.diagnostics.push(
                    Diagnostic::warning(format!("`{name}` is never used"), *span)
                        .with_label("unused variable")
                        .with_help(format!("Prefix with underscore `_{name}` to suppress"))
                        .with_code("W001"),
                );
            }
        }

        // Build type map from the top-level scope
        let type_map: HashMap<String, String> = self
            .env
            .scopes
            .iter()
            .flat_map(|scope| scope.iter())
            .map(|(name, ty)| (name.clone(), ty.display_name()))
            .collect();

        (self.diagnostics, type_map)
    }

    fn fresh_type_var(&mut self) -> Type {
        let id = self.next_var;
        self.next_var += 1;
        Type::Var(id)
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
            TypeExprKind::Named { name, type_args } => {
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
            "bool" => Type::Bool,
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
                            .with_help("Check the spelling or import/define this type")
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
            ItemKind::Expr(expr) => {
                let ty = self.check_expr(expr);
                // Rule 5: No floating Results/Options
                if ty.is_result() {
                    self.diagnostics.push(
                        Diagnostic::error("unhandled Result", expr.span)
                            .with_label("this Result is not used")
                            .with_help("Use `?`, `match`, or assign to `_`")
                            .with_code("E005"),
                    );
                }
            }
        }
    }

    fn check_import(&mut self, decl: &ImportDecl) {
        for spec in &decl.specifiers {
            let effective_name = spec.alias.as_deref().unwrap_or(&spec.name);
            self.env.define(effective_name, Type::Unknown);
            self.imported_names
                .push((effective_name.to_string(), spec.span));

            // Track untrusted imports (not trusted at module or specifier level)
            if !decl.trusted && !spec.trusted {
                self.untrusted_imports.insert(effective_name.to_string());
            }
        }
    }

    fn check_const(&mut self, decl: &ConstDecl, span: Span) {
        let value_type = self.check_expr(&decl.value);

        let declared_type = decl.type_ann.as_ref().map(|t| self.resolve_type(t));

        let final_type = if let Some(ref declared) = declared_type {
            if !self.types_compatible(declared, &value_type) {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "type mismatch: expected `{}`, found `{}`",
                            declared.display_name(),
                            value_type.display_name()
                        ),
                        span,
                    )
                    .with_label("type mismatch")
                    .with_code("E001"),
                );
            }
            declared.clone()
        } else {
            value_type
        };

        match &decl.binding {
            ConstBinding::Name(name) => {
                self.env.define(name, final_type);
                if decl.exported {
                    self.used_names.insert(name.clone());
                }
                self.defined_names.push((name.clone(), span));
            }
            ConstBinding::Array(names) => {
                for name in names {
                    self.env.define(name, Type::Unknown);
                    self.defined_names.push((name.clone(), span));
                }
            }
            ConstBinding::Object(names) => {
                for name in names {
                    self.env.define(name, Type::Unknown);
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
                .with_help("Add `: ReturnType` after the parameter list")
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
        self.env.define(&decl.name, fn_type);
        if decl.exported {
            self.used_names.insert(decl.name.clone());
        }
        self.defined_names.push((decl.name.clone(), span));

        // Set up scope for function body
        let prev_return_type = self.current_return_type.take();
        self.current_return_type = Some(return_type.clone());

        self.env.push_scope();

        // Define parameters
        for (param, ty) in decl.params.iter().zip(param_types.iter()) {
            self.env.define(&param.name, ty.clone());
        }

        // Check body
        let body_type = self.check_expr(&decl.body);

        // Check return type compatibility
        if let Some(ref declared_return) = decl.return_type {
            let resolved = self.resolve_type(declared_return);
            if !self.types_compatible(&resolved, &body_type) && !matches!(body_type, Type::Var(_)) {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "function `{}` return type mismatch: expected `{}`, found `{}`",
                            decl.name,
                            resolved.display_name(),
                            body_type.display_name()
                        ),
                        span,
                    )
                    .with_label("return type mismatch")
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
                    .with_help("Add a return expression or change return type to `()`")
                    .with_code("E013"),
                );
            }
        }

        self.env.pop_scope();
        self.current_return_type = prev_return_type;
    }

    /// Checks if a function body contains a return expression.
    fn body_has_return(&self, body: &Expr) -> bool {
        match &body.kind {
            ExprKind::Return(Some(_)) => true,
            ExprKind::Block(items) => items.iter().any(|item| {
                if let ItemKind::Expr(e) = &item.kind {
                    self.body_has_return(e)
                } else {
                    false
                }
            }),
            ExprKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.body_has_return(then_branch)
                    && else_branch
                        .as_ref()
                        .is_some_and(|e| self.body_has_return(e))
            }
            ExprKind::Match { arms, .. } => {
                !arms.is_empty() && arms.iter().all(|arm| self.body_has_return(&arm.body))
            }
            _ => false,
        }
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
                            .with_label("unknown component")
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
