use std::collections::{HashMap, HashSet};

use crate::diagnostic::Diagnostic;
use crate::lexer::span::Span;
use crate::parser::ast::*;

// ── Types ────────────────────────────────────────────────────────

/// Internal type representation used by the checker.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Primitive types: number, string, bool
    Number,
    String,
    Bool,
    /// The undefined type (used for None)
    Undefined,
    /// A named/user-defined type
    Named(String),
    /// Brand type: distinct at compile time, erases to base at runtime
    Brand {
        base: Box<Type>,
        tag: String,
    },
    /// Opaque type: only the defining module can construct/destructure
    Opaque {
        name: String,
        base: Box<Type>,
    },
    /// Result<T, E>
    Result {
        ok: Box<Type>,
        err: Box<Type>,
    },
    /// Option<T> = T | undefined
    Option(Box<Type>),
    /// Function type
    Function {
        params: Vec<Type>,
        return_type: Box<Type>,
    },
    /// Array type
    Array(Box<Type>),
    /// Tuple type
    Tuple(Vec<Type>),
    /// Record/struct type
    Record(Vec<(String, Type)>),
    /// Union (tagged discriminated union)
    Union {
        name: String,
        variants: Vec<(String, Vec<Type>)>,
    },
    /// Type variable (for inference)
    Var(usize),
    /// The unknown/any escape hatch
    Unknown,
    /// Void (no return value)
    Void,
    /// JSX element type
    JsxElement,
}

impl Type {
    fn is_result(&self) -> bool {
        matches!(self, Type::Result { .. })
    }

    fn is_option(&self) -> bool {
        matches!(self, Type::Option(_))
    }

    fn is_numeric(&self) -> bool {
        matches!(self, Type::Number)
    }

    fn display_name(&self) -> String {
        match self {
            Type::Number => "number".to_string(),
            Type::String => "string".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Undefined => "undefined".to_string(),
            Type::Named(n) => n.clone(),
            Type::Brand { tag, .. } => tag.clone(),
            Type::Opaque { name, .. } => name.clone(),
            Type::Result { ok, err } => {
                format!("Result<{}, {}>", ok.display_name(), err.display_name())
            }
            Type::Option(inner) => format!("Option<{}>", inner.display_name()),
            Type::Function {
                params,
                return_type,
            } => {
                let p: Vec<_> = params.iter().map(|t| t.display_name()).collect();
                format!("({}) => {}", p.join(", "), return_type.display_name())
            }
            Type::Array(inner) => format!("Array<{}>", inner.display_name()),
            Type::Tuple(types) => {
                let t: Vec<_> = types.iter().map(|t| t.display_name()).collect();
                format!("[{}]", t.join(", "))
            }
            Type::Record(fields) => {
                let f: Vec<_> = fields
                    .iter()
                    .map(|(n, t)| format!("{n}: {}", t.display_name()))
                    .collect();
                format!("{{ {} }}", f.join(", "))
            }
            Type::Union { name, .. } => name.clone(),
            Type::Var(id) => format!("?T{id}"),
            Type::Unknown => "unknown".to_string(),
            Type::Void => "void".to_string(),
            Type::JsxElement => "JSX.Element".to_string(),
        }
    }
}

// ── Type Environment ─────────────────────────────────────────────

/// Tracks types of variables, functions, and type declarations in scope.
#[derive(Debug, Clone)]
struct TypeEnv {
    /// Stack of scopes (innermost last). Each scope maps names to types.
    scopes: Vec<HashMap<String, Type>>,
    /// Type declarations: type name -> TypeDef + metadata
    type_defs: HashMap<String, TypeInfo>,
}

#[derive(Debug, Clone)]
struct TypeInfo {
    #[allow(dead_code)]
    def: TypeDef,
    opaque: bool,
    #[allow(dead_code)]
    type_params: Vec<String>,
}

impl TypeEnv {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            type_defs: HashMap::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    fn define_type(&mut self, name: &str, info: TypeInfo) {
        self.type_defs.insert(name.to_string(), info);
    }

    fn lookup_type(&self, name: &str) -> Option<&TypeInfo> {
        self.type_defs.get(name)
    }
}

// ── Checker ──────────────────────────────────────────────────────

/// The ZenScript type checker.
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
        for item in &program.items {
            if let ItemKind::TypeDecl(decl) = &item.kind {
                self.register_type_decl(decl);
            }
        }

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

    fn resolve_type(&mut self, type_expr: &TypeExpr) -> Type {
        match &type_expr.kind {
            TypeExprKind::Named { name, type_args } => self.resolve_named_type(name, type_args),
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

    fn resolve_named_type(&mut self, name: &str, type_args: &[TypeExpr]) -> Type {
        match name {
            "number" => Type::Number,
            "string" => Type::String,
            "bool" => Type::Bool,
            "void" => Type::Void,
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
            _ => Type::Named(name.to_string()),
        }
    }

    // ── Item Checking ────────────────────────────────────────────

    fn check_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Import(decl) => self.check_import(decl),
            ItemKind::Const(decl) => self.check_const(decl, item.span),
            ItemKind::Function(decl) => self.check_function(decl, item.span),
            ItemKind::TypeDecl(_) => {} // already registered in first pass
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
        }

        self.env.pop_scope();
        self.current_return_type = prev_return_type;
    }

    // ── Expression Checking ──────────────────────────────────────

    fn check_expr(&mut self, expr: &Expr) -> Type {
        match &expr.kind {
            ExprKind::Number(_) => Type::Number,
            ExprKind::String(_) => Type::String,
            ExprKind::TemplateLiteral(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(e) = part {
                        self.check_expr(e);
                    }
                }
                Type::String
            }
            ExprKind::Bool(_) => Type::Bool,

            ExprKind::Identifier(name) => {
                self.used_names.insert(name.clone());
                if let Some(ty) = self.env.lookup(name).cloned() {
                    ty
                } else {
                    self.diagnostics.push(
                        Diagnostic::error(format!("`{name}` is not defined"), expr.span)
                            .with_label("not found in scope")
                            .with_code("E002"),
                    );
                    Type::Unknown
                }
            }

            ExprKind::Placeholder => Type::Unknown,

            ExprKind::Binary { left, op, right } => self.check_binary(left, *op, right, expr.span),

            ExprKind::Unary { op, operand } => {
                let ty = self.check_expr(operand);
                match op {
                    UnaryOp::Neg => {
                        if !ty.is_numeric() && !matches!(ty, Type::Unknown | Type::Var(_)) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!("cannot negate `{}`", ty.display_name()),
                                    expr.span,
                                )
                                .with_label("not a number")
                                .with_code("E001"),
                            );
                        }
                        Type::Number
                    }
                    UnaryOp::Not => Type::Bool,
                }
            }

            ExprKind::Pipe { left, right } => {
                let _left_ty = self.check_expr(left);
                self.check_expr(right)
            }

            ExprKind::Unwrap(inner) => {
                let ty = self.check_expr(inner);
                // Rule 5: ? only allowed in functions returning Result/Option
                match &self.current_return_type {
                    Some(ret) if ret.is_result() || ret.is_option() => {}
                    Some(_) => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "? operator requires function to return Result or Option",
                                expr.span,
                            )
                            .with_label("invalid ? usage")
                            .with_help("Change the function's return type to Result or Option")
                            .with_code("E005"),
                        );
                    }
                    None => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "? operator can only be used inside a function",
                                expr.span,
                            )
                            .with_label("not inside a function")
                            .with_code("E005"),
                        );
                    }
                }
                // Unwrap the inner type
                match ty {
                    Type::Result { ok, .. } => *ok,
                    Type::Option(inner) => *inner,
                    _ => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!(
                                    "? can only be used on Result or Option, found `{}`",
                                    ty.display_name()
                                ),
                                expr.span,
                            )
                            .with_label("not a Result or Option")
                            .with_code("E005"),
                        );
                        Type::Unknown
                    }
                }
            }

            ExprKind::Call { callee, args } => {
                let callee_ty = self.check_expr(callee);
                for arg in args {
                    match arg {
                        Arg::Positional(e) | Arg::Named { value: e, .. } => {
                            self.check_expr(e);
                        }
                    }
                }
                match callee_ty {
                    Type::Function { return_type, .. } => *return_type,
                    _ => Type::Unknown,
                }
            }

            ExprKind::Construct {
                type_name,
                spread,
                args,
            } => {
                self.used_names.insert(type_name.clone());

                let type_info = self.env.lookup_type(type_name).cloned();
                if type_info.is_none() {
                    self.diagnostics.push(
                        Diagnostic::error(format!("unknown type `{type_name}`"), expr.span)
                            .with_label("not a known type")
                            .with_code("E002"),
                    );
                }

                // Rule 3: Opaque enforcement
                if let Some(ref info) = type_info
                    && info.opaque
                {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot construct opaque type `{type_name}` outside its defining module"
                            ),
                            expr.span,
                        )
                        .with_label("opaque type")
                        .with_help("Use the module's exported constructor function instead")
                        .with_code("E003"),
                    );
                }

                if let Some(spread_expr) = spread {
                    self.check_expr(spread_expr);
                }
                for arg in args {
                    match arg {
                        Arg::Positional(e) | Arg::Named { value: e, .. } => {
                            self.check_expr(e);
                        }
                    }
                }

                Type::Named(type_name.clone())
            }

            ExprKind::Member { object, field } => {
                let obj_ty = self.check_expr(object);
                // Rule 6: No property access on unnarrowed unions
                if let Type::Result { .. } = obj_ty {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot access `.{field}` on Result - use `match` or `?` first"
                            ),
                            expr.span,
                        )
                        .with_label("Result not narrowed")
                        .with_help("Use `match result { Ok(v) -> ..., Err(e) -> ... }`")
                        .with_code("E006"),
                    );
                }
                if let Type::Union { name, .. } = &obj_ty {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot access `.{field}` on union `{name}` - use `match` first"
                            ),
                            expr.span,
                        )
                        .with_label("union not narrowed")
                        .with_help("Use `match` to narrow the union first")
                        .with_code("E006"),
                    );
                }
                if let Type::Record(fields) = &obj_ty
                    && let Some((_, ty)) = fields.iter().find(|(n, _)| n == field)
                {
                    return ty.clone();
                }
                Type::Unknown
            }

            ExprKind::Index { object, index } => {
                let obj_ty = self.check_expr(object);
                self.check_expr(index);
                if let Type::Array(inner) = obj_ty {
                    Type::Option(inner)
                } else {
                    Type::Unknown
                }
            }

            ExprKind::Arrow { params, body } => {
                self.env.push_scope();
                let param_types: Vec<_> = params
                    .iter()
                    .map(|p| {
                        let ty = p
                            .type_ann
                            .as_ref()
                            .map(|t| self.resolve_type(t))
                            .unwrap_or_else(|| self.fresh_type_var());
                        self.env.define(&p.name, ty.clone());
                        ty
                    })
                    .collect();
                let return_type = self.check_expr(body);
                self.env.pop_scope();
                Type::Function {
                    params: param_types,
                    return_type: Box::new(return_type),
                }
            }

            ExprKind::Match { subject, arms } => {
                let subject_ty = self.check_expr(subject);
                self.check_match_exhaustiveness(&subject_ty, arms, expr.span);

                let mut result_type: Option<Type> = None;
                for arm in arms {
                    self.env.push_scope();
                    self.check_pattern(&arm.pattern, &subject_ty);
                    let arm_type = self.check_expr(&arm.body);
                    self.env.pop_scope();

                    if result_type.is_none() {
                        result_type = Some(arm_type);
                    }
                }
                result_type.unwrap_or(Type::Void)
            }

            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expr(condition);
                let then_ty = self.check_expr(then_branch);
                if let Some(else_expr) = else_branch {
                    self.check_expr(else_expr);
                }
                then_ty
            }

            ExprKind::Return(value) => {
                if let Some(e) = value {
                    self.check_expr(e);
                }
                Type::Void
            }

            ExprKind::Await(inner) => self.check_expr(inner),

            ExprKind::Ok(inner) => {
                let inner_ty = self.check_expr(inner);
                Type::Result {
                    ok: Box::new(inner_ty),
                    err: Box::new(Type::Unknown),
                }
            }

            ExprKind::Err(inner) => {
                let err_ty = self.check_expr(inner);
                Type::Result {
                    ok: Box::new(Type::Unknown),
                    err: Box::new(err_ty),
                }
            }

            ExprKind::Some(inner) => {
                let inner_ty = self.check_expr(inner);
                Type::Option(Box::new(inner_ty))
            }

            ExprKind::None => Type::Option(Box::new(Type::Unknown)),

            ExprKind::Jsx(element) => {
                self.check_jsx(element);
                Type::JsxElement
            }

            ExprKind::Block(items) => {
                self.env.push_scope();
                let mut last_type = Type::Void;
                let mut found_return = false;
                for (i, item) in items.iter().enumerate() {
                    if found_return {
                        // Rule 10: Dead code detection
                        self.diagnostics.push(
                            Diagnostic::error("unreachable code", item.span)
                                .with_label("this code is unreachable")
                                .with_help("Remove this code")
                                .with_code("E011"),
                        );
                        break;
                    }
                    self.check_item(item);
                    if let ItemKind::Expr(expr) = &item.kind {
                        if matches!(expr.kind, ExprKind::Return(_)) {
                            found_return = true;
                        }
                        if i == items.len() - 1 {
                            last_type = self.check_expr(expr);
                        }
                    }
                }
                self.env.pop_scope();
                last_type
            }

            ExprKind::Grouped(inner) => self.check_expr(inner),

            ExprKind::Array(elements) => {
                let mut elem_type: Option<Type> = None;
                for el in elements {
                    let ty = self.check_expr(el);
                    if let Some(ref prev) = elem_type {
                        if !self.types_compatible(prev, &ty)
                            && !matches!(ty, Type::Unknown | Type::Var(_))
                            && !matches!(prev, Type::Unknown | Type::Var(_))
                        {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "mixed array needs explicit type annotation",
                                    el.span,
                                )
                                .with_label("mismatched element type")
                                .with_help("Add an explicit type annotation to the array")
                                .with_code("E004"),
                            );
                        }
                    } else {
                        elem_type = Some(ty);
                    }
                }
                Type::Array(Box::new(elem_type.unwrap_or(Type::Unknown)))
            }

            ExprKind::Spread(inner) => self.check_expr(inner),
        }
    }

    // ── Binary Expression Checking ───────────────────────────────

    fn check_binary(&mut self, left: &Expr, op: BinOp, right: &Expr, span: Span) -> Type {
        let left_ty = self.check_expr(left);
        let right_ty = self.check_expr(right);

        match op {
            // Rule 8: == only between same types
            BinOp::Eq | BinOp::NotEq => {
                if !self.types_compatible(&left_ty, &right_ty)
                    && !matches!(left_ty, Type::Unknown | Type::Var(_))
                    && !matches!(right_ty, Type::Unknown | Type::Var(_))
                {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot compare `{}` with `{}`",
                                left_ty.display_name(),
                                right_ty.display_name()
                            ),
                            span,
                        )
                        .with_label("type mismatch in comparison")
                        .with_help("Convert one side to match the other")
                        .with_code("E008"),
                    );
                }
                // Rule 2: Brand enforcement
                if let (Type::Brand { tag: tag_l, .. }, Type::Brand { tag: tag_r, .. }) =
                    (&left_ty, &right_ty)
                    && tag_l != tag_r
                {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot compare `{tag_l}` with `{tag_r}` - different branded types"
                            ),
                            span,
                        )
                        .with_label("brand mismatch")
                        .with_help(format!(
                            "`{tag_l}` and `{tag_r}` are distinct types even though they share the same base type"
                        ))
                        .with_code("E002"),
                    );
                }
                Type::Bool
            }
            BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => Type::Bool,
            BinOp::And | BinOp::Or => Type::Bool,
            BinOp::Add => {
                // Rule 12: String concat with + warning
                if matches!(left_ty, Type::String) || matches!(right_ty, Type::String) {
                    self.diagnostics.push(
                        Diagnostic::warning(
                            "use template literal instead of `+` for string concatenation",
                            span,
                        )
                        .with_label("string concat with +")
                        .with_help("Use `${a}${b}` instead")
                        .with_code("W002"),
                    );
                }
                if matches!(left_ty, Type::Number) && matches!(right_ty, Type::Number) {
                    Type::Number
                } else if matches!(left_ty, Type::String) || matches!(right_ty, Type::String) {
                    Type::String
                } else {
                    left_ty
                }
            }
            BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => Type::Number,
        }
    }

    // ── Match Exhaustiveness ─────────────────────────────────────

    fn check_match_exhaustiveness(&mut self, subject_ty: &Type, arms: &[MatchArm], span: Span) {
        let has_catch_all = arms.iter().any(|arm| {
            matches!(
                arm.pattern.kind,
                PatternKind::Wildcard | PatternKind::Binding(_)
            )
        });

        if has_catch_all {
            return;
        }

        // For union types, check that all variants are covered
        if let Type::Union { name, variants } = subject_ty {
            let variant_names: HashSet<&str> = variants.iter().map(|(n, _)| n.as_str()).collect();
            let mut covered: HashSet<&str> = HashSet::new();

            for arm in arms {
                if let PatternKind::Variant { name, .. } = &arm.pattern.kind {
                    covered.insert(name.as_str());
                }
            }

            let missing: Vec<_> = variant_names.difference(&covered).collect();
            if !missing.is_empty() {
                let missing_str = missing
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("non-exhaustive match on `{name}` - missing: {missing_str}"),
                        span,
                    )
                    .with_label("not all variants covered")
                    .with_help("Add match arms for the missing variants, or add a `_ ->` catch-all")
                    .with_code("E004"),
                );
            }
        }

        // For Result types, check Ok and Err are covered
        if subject_ty.is_result() {
            let mut has_ok = false;
            let mut has_err = false;
            for arm in arms {
                if let PatternKind::Variant { name, .. } = &arm.pattern.kind {
                    match name.as_str() {
                        "Ok" => has_ok = true,
                        "Err" => has_err = true,
                        _ => {}
                    }
                }
            }
            if !has_ok || !has_err {
                let missing = match (has_ok, has_err) {
                    (false, false) => "`Ok` and `Err`",
                    (false, true) => "`Ok`",
                    (true, false) => "`Err`",
                    _ => unreachable!(),
                };
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("non-exhaustive match on Result - missing: {missing}"),
                        span,
                    )
                    .with_label("not all cases covered")
                    .with_help("Add match arms for the missing cases")
                    .with_code("E004"),
                );
            }
        }

        // For Option types, check Some and None are covered
        if subject_ty.is_option() {
            let mut has_some = false;
            let mut has_none = false;
            for arm in arms {
                match &arm.pattern.kind {
                    PatternKind::Variant { name, .. } if name == "Some" => has_some = true,
                    PatternKind::Variant { name, .. } if name == "None" => has_none = true,
                    _ => {}
                }
            }
            if !has_some || !has_none {
                let missing = match (has_some, has_none) {
                    (false, false) => "`Some` and `None`",
                    (false, true) => "`Some`",
                    (true, false) => "`None`",
                    _ => unreachable!(),
                };
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("non-exhaustive match on Option - missing: {missing}"),
                        span,
                    )
                    .with_label("not all cases covered")
                    .with_help("Add match arms for the missing cases")
                    .with_code("E004"),
                );
            }
        }

        // For bool, check true/false covered
        if matches!(subject_ty, Type::Bool) {
            let mut has_true = false;
            let mut has_false = false;
            for arm in arms {
                if let PatternKind::Literal(LiteralPattern::Bool(b)) = &arm.pattern.kind {
                    if *b {
                        has_true = true;
                    } else {
                        has_false = true;
                    }
                }
            }
            if !has_true || !has_false {
                self.diagnostics.push(
                    Diagnostic::error("non-exhaustive match on bool - missing a case", span)
                        .with_label("not all cases covered")
                        .with_help("Add match arms for both `true` and `false`")
                        .with_code("E004"),
                );
            }
        }
    }

    // ── Pattern Checking ─────────────────────────────────────────

    fn check_pattern(&mut self, pattern: &Pattern, subject_ty: &Type) {
        match &pattern.kind {
            PatternKind::Literal(_) | PatternKind::Range { .. } | PatternKind::Wildcard => {}
            PatternKind::Variant { name, fields } => {
                if let Type::Union { variants, .. } = subject_ty
                    && let Some((_, field_types)) = variants.iter().find(|(n, _)| n == name)
                {
                    for (pat, ty) in fields.iter().zip(field_types.iter()) {
                        self.check_pattern(pat, ty);
                    }
                }
                if let Type::Result { ok, err } = subject_ty {
                    match name.as_str() {
                        "Ok" => {
                            if let Some(pat) = fields.first() {
                                self.check_pattern(pat, ok);
                            }
                        }
                        "Err" => {
                            if let Some(pat) = fields.first() {
                                self.check_pattern(pat, err);
                            }
                        }
                        _ => {}
                    }
                }
                if let Type::Option(inner) = subject_ty
                    && name == "Some"
                    && let Some(pat) = fields.first()
                {
                    self.check_pattern(pat, inner);
                }
            }
            PatternKind::Record { fields } => {
                for (_, pat) in fields {
                    self.check_pattern(pat, &Type::Unknown);
                }
            }
            PatternKind::Binding(name) => {
                self.env.define(name, subject_ty.clone());
            }
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
            | (Type::Void, Type::Void)
            | (Type::Undefined, Type::Undefined)
            | (Type::JsxElement, Type::JsxElement) => true,
            (Type::Named(a), Type::Named(b)) => a == b,
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

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use crate::parser::Parser;

    fn check(source: &str) -> Vec<Diagnostic> {
        let program = Parser::new(source)
            .parse_program()
            .expect("parse should succeed");
        Checker::new().check(&program)
    }

    fn has_error(diagnostics: &[Diagnostic], code: &str) -> bool {
        diagnostics.iter().any(|d| d.code.as_deref() == Some(code))
    }

    fn has_error_containing(diagnostics: &[Diagnostic], text: &str) -> bool {
        diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.message.contains(text))
    }

    fn has_warning_containing(diagnostics: &[Diagnostic], text: &str) -> bool {
        diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains(text))
    }

    // ── Rule 1: Basic type checking ─────────────────────────────

    #[test]
    fn basic_const_number() {
        let diags = check("const x = 42");
        assert!(!has_error(&diags, "E001"));
    }

    #[test]
    fn basic_const_string() {
        let diags = check("const x = \"hello\"");
        assert!(!has_error(&diags, "E001"));
    }

    #[test]
    fn undeclared_variable() {
        let diags = check("const x = y");
        assert!(has_error_containing(&diags, "`y` is not defined"));
    }

    // ── Rule 2: Brand enforcement ───────────────────────────────

    #[test]
    fn brand_comparison_different_tags() {
        let diags = check(
            r#"
type UserId = Brand<string, UserId>
type Email = Brand<string, Email>
const a: UserId = UserId("abc")
const b: Email = Email("test@test.com")
const result = a == b
"#,
        );
        assert!(has_error_containing(&diags, "cannot compare"));
    }

    // ── Rule 4: Exhaustiveness checking ─────────────────────────

    #[test]
    fn exhaustive_match_with_wildcard() {
        let diags = check(
            r#"
const x = match 42 {
    1 -> "one",
    _ -> "other",
}
"#,
        );
        assert!(!has_error(&diags, "E004"));
    }

    #[test]
    fn non_exhaustive_bool_match() {
        let diags = check(
            r#"
const x: bool = true
const y = match x {
    true -> "yes",
}
"#,
        );
        assert!(has_error_containing(&diags, "non-exhaustive"));
    }

    // ── Rule 5: Result/Option ? tracking ────────────────────────

    #[test]
    fn unwrap_in_result_function() {
        let diags = check(
            r#"
function tryFetch(url: string): Result<string, string> {
    const result = Ok("data")
    const value = result?
    return Ok(value)
}
"#,
        );
        let unwrap_errors: Vec<_> = diags
            .iter()
            .filter(|d| {
                d.code.as_deref() == Some("E005") && d.message.contains("? operator requires")
            })
            .collect();
        assert!(unwrap_errors.is_empty());
    }

    #[test]
    fn unwrap_not_on_result_or_option() {
        let diags = check(
            r#"
function process(): Result<number, string> {
    const x = 42
    const y = x?
    return Ok(y)
}
"#,
        );
        assert!(has_error_containing(
            &diags,
            "? can only be used on Result or Option"
        ));
    }

    // ── Rule 6: No property access on unnarrowed unions ─────────

    #[test]
    fn property_access_on_result() {
        let diags = check(
            r#"
const result = Ok(42)
const x = result.value
"#,
        );
        assert!(has_error_containing(
            &diags,
            "cannot access `.value` on Result"
        ));
    }

    // ── Rule 8: Same-type equality ──────────────────────────────

    #[test]
    fn equality_same_types() {
        let diags = check("const x = 1 == 1");
        assert!(!has_error(&diags, "E008"));
    }

    #[test]
    fn equality_different_types() {
        let diags = check(r#"const x = 1 == "hello""#);
        assert!(has_error_containing(&diags, "cannot compare"));
    }

    // ── Rule 9: Unused detection ────────────────────────────────

    #[test]
    fn unused_variable_warning() {
        let diags = check("const x = 42");
        assert!(has_warning_containing(&diags, "is never used"));
    }

    #[test]
    fn underscore_prefix_suppresses_unused() {
        let diags = check("const _x = 42");
        assert!(!has_warning_containing(&diags, "is never used"));
    }

    #[test]
    fn used_variable_no_warning() {
        let diags = check(
            r#"
const x = 42
const y = x
"#,
        );
        let unused_x: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Warning && d.message.contains("`x`"))
            .collect();
        assert!(unused_x.is_empty());
    }

    #[test]
    fn unused_import_error() {
        let diags = check(r#"import { useState } from "react""#);
        assert!(has_error_containing(&diags, "is never used"));
    }

    // ── Rule 10: Exported function return types ─────────────────

    #[test]
    fn exported_function_needs_return_type() {
        let diags = check("export function add(a: number, b: number) { return a }");
        assert!(has_error_containing(&diags, "must declare a return type"));
    }

    #[test]
    fn exported_function_with_return_type_ok() {
        let diags = check("export function add(a: number, b: number): number { return a }");
        assert!(!has_error(&diags, "E010"));
    }

    // ── Rule 12: String concat warning ──────────────────────────

    #[test]
    fn string_concat_warning() {
        let diags = check(r#"const x = "hello" + " world""#);
        assert!(has_warning_containing(&diags, "template literal"));
    }

    // ── OK/Err/Some/None types ──────────────────────────────────

    #[test]
    fn ok_creates_result() {
        let diags = check("const _x = Ok(42)");
        assert!(!has_error(&diags, "E001"));
    }

    #[test]
    fn none_creates_option() {
        let diags = check("const _x = None");
        assert!(!has_error(&diags, "E001"));
    }

    // ── Array type checking ─────────────────────────────────────

    #[test]
    fn homogeneous_array() {
        let diags = check("const _x = [1, 2, 3]");
        assert!(!has_error(&diags, "E004"));
    }

    #[test]
    fn mixed_array_error() {
        let diags = check(r#"const _x = [1, "two", 3]"#);
        assert!(has_error_containing(&diags, "mixed array"));
    }

    // ── Dead code detection ─────────────────────────────────────

    #[test]
    fn dead_code_after_return() {
        let diags = check(
            r#"
function test(): number {
    return 1
    const x = 2
}
"#,
        );
        assert!(has_error_containing(&diags, "unreachable code"));
    }

    // ── Opaque type enforcement ─────────────────────────────────

    #[test]
    fn opaque_type_cannot_be_constructed() {
        let diags = check(
            r#"
opaque type HashedPassword = string
const _x = HashedPassword("abc")
"#,
        );
        assert!(has_error_containing(&diags, "opaque type"));
    }

    // ── Unhandled Result ────────────────────────────────────────

    #[test]
    fn floating_result_error() {
        let diags = check("Ok(42)");
        assert!(has_error_containing(&diags, "unhandled Result"));
    }
}
