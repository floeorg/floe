use std::collections::HashMap;
use std::fmt;

use crate::parser::ast::TypeDef;

// ── Types ────────────────────────────────────────────────────────

/// Internal type representation used by the checker.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Primitive types: number, string, boolean
    Number,
    String,
    Bool,
    /// The undefined type (used for None)
    Undefined,
    /// A named/user-defined type (locally defined in Floe)
    Named(String),
    /// A foreign type from npm imports — structure unknown to Floe,
    /// but TypeScript validated it at the source
    Foreign(String),
    /// Promise<T> — async return type, unwrapped by `Promise.await`
    Promise(Box<Type>),
    /// Opaque type: only the defining module can construct/destructure
    Opaque {
        name: String,
        base: Box<Type>,
    },
    // Result<T, E> is now represented as Type::Union { name: "Result", variants: [("Ok", [T]), ("Err", [E])] }
    // Use Type::result_of(ok, err) to construct, ty.is_result() / ty.result_ok() / ty.result_err() to inspect.
    // Option<T> is now represented as Type::Union { name: "Option", variants: [("Some", [T]), ("None", [])] }
    // Use Type::option_of(inner) to construct, ty.is_option() / ty.option_inner() to inspect.
    /// Settable<T> = Set(T) | Clear | Unchanged
    Settable(Box<Type>),
    /// Function type. `required_params` is how many leading params the caller
    /// must provide; trailing params beyond that index have defaults and can be omitted.
    Function {
        params: Vec<Type>,
        required_params: usize,
        return_type: Box<Type>,
    },
    /// Array type
    Array(Box<Type>),
    /// Map type: Map<K, V> (JS Map at runtime)
    Map {
        key: Box<Type>,
        value: Box<Type>,
    },
    /// TS Record<K, V> — plain object at runtime, supports Map operations
    /// via bracket access codegen instead of JS Map API
    RecordMap {
        key: Box<Type>,
        value: Box<Type>,
    },
    /// Set type: Set<T>
    Set {
        element: Box<Type>,
    },
    /// Tuple type
    Tuple(Vec<Type>),
    /// Record/struct type
    Record(Vec<(String, Type)>),
    /// Union (tagged discriminated union)
    Union {
        name: String,
        variants: Vec<(String, Vec<Type>)>,
    },
    /// Untagged union: `Date | number | string` or `"GET" | "POST"`
    /// Compatible if the value matches any member type.
    TsUnion(Vec<Type>),
    /// String literal type: `"GET"`, `"POST"`, etc.
    StringLiteral(String),
    /// Type variable (for inference)
    Var(usize),
    /// The unknown/any escape hatch — used for genuinely unknown external types
    /// (e.g. npm imports without type probes). Compatible with everything as expected,
    /// but not as actual (same as TypeScript's `unknown`).
    Unknown,
    /// Error sentinel — type resolution failed and an error was already emitted.
    /// Compatible with all types in both directions to suppress cascading errors.
    /// Should never reach codegen; if it does, that indicates a checker bug.
    Error,
    /// Unit type () — replaces void, a real value usable in generics
    Unit,
    /// The never type — used for `todo` and `unreachable`, compatible with any type
    Never,
}

impl Type {
    /// Construct an Option<T> as a Union type.
    pub fn option_of(inner: Type) -> Type {
        Type::Union {
            name: crate::type_layout::TYPE_OPTION.to_string(),
            variants: vec![
                (crate::type_layout::VARIANT_SOME.to_string(), vec![inner]),
                (crate::type_layout::VARIANT_NONE.to_string(), vec![]),
            ],
        }
    }

    /// Construct a Result<T, E> as a Union type.
    pub fn result_of(ok: Type, err: Type) -> Type {
        Type::Union {
            name: crate::type_layout::TYPE_RESULT.to_string(),
            variants: vec![
                (crate::type_layout::VARIANT_OK.to_string(), vec![ok]),
                (crate::type_layout::VARIANT_ERR.to_string(), vec![err]),
            ],
        }
    }

    pub(crate) fn is_result(&self) -> bool {
        matches!(
            self,
            Type::Union { name, .. } if name == crate::type_layout::TYPE_RESULT
        )
    }

    /// Extract T from Result<T, E> (the Union representation). Returns None if not a Result.
    pub fn result_ok(&self) -> Option<&Type> {
        if let Type::Union { name, variants } = self
            && name == crate::type_layout::TYPE_RESULT
        {
            variants
                .iter()
                .find(|(n, _)| n == crate::type_layout::VARIANT_OK)
                .and_then(|(_, fields)| fields.first())
        } else {
            None
        }
    }

    /// Extract E from Result<T, E> (the Union representation). Returns None if not a Result.
    pub fn result_err(&self) -> Option<&Type> {
        if let Type::Union { name, variants } = self
            && name == crate::type_layout::TYPE_RESULT
        {
            variants
                .iter()
                .find(|(n, _)| n == crate::type_layout::VARIANT_ERR)
                .and_then(|(_, fields)| fields.first())
        } else {
            None
        }
    }

    pub(crate) fn is_option(&self) -> bool {
        matches!(
            self,
            Type::Union { name, .. } if name == crate::type_layout::TYPE_OPTION
        )
    }

    pub(crate) fn is_settable(&self) -> bool {
        matches!(self, Type::Settable(_))
    }

    /// Extract T from Option<T> (the Union representation). Returns None if not an Option.
    pub fn option_inner(&self) -> Option<&Type> {
        if let Type::Union { name, variants } = self
            && name == crate::type_layout::TYPE_OPTION
        {
            variants
                .iter()
                .find(|(n, _)| n == crate::type_layout::VARIANT_SOME)
                .and_then(|(_, fields)| fields.first())
        } else {
            None
        }
    }

    /// Unwrap Option<T> → T. If not an Option, return self.
    pub fn unwrap_option(self) -> Type {
        if let Type::Union { name, variants } = self {
            if name == crate::type_layout::TYPE_OPTION {
                return variants
                    .into_iter()
                    .find(|(n, _)| n == crate::type_layout::VARIANT_SOME)
                    .and_then(|(_, mut fields)| fields.pop())
                    .unwrap_or(Type::Unknown);
            }
            Type::Union { name, variants }
        } else {
            self
        }
    }

    pub(crate) fn is_numeric(&self) -> bool {
        matches!(self, Type::Number)
    }

    pub(crate) fn is_boolean(&self) -> bool {
        matches!(self, Type::Bool)
    }

    pub(crate) fn is_primitive(&self) -> bool {
        matches!(
            self,
            Type::Number
                | Type::String
                | Type::Bool
                | Type::Unit
                | Type::Undefined
                | Type::StringLiteral(_)
        )
    }

    /// Check if this is a TsUnion of all StringLiterals (non-allocating).
    pub(crate) fn is_string_literal_union(&self) -> bool {
        if let Type::TsUnion(members) = self {
            !members.is_empty() && members.iter().all(|m| matches!(m, Type::StringLiteral(_)))
        } else {
            false
        }
    }

    /// If this is a TsUnion of all StringLiterals, return the string values.
    pub(crate) fn as_string_literal_variants(&self) -> Option<Vec<&str>> {
        if let Type::TsUnion(members) = self {
            let strings: Vec<&str> = members
                .iter()
                .filter_map(|m| {
                    if let Type::StringLiteral(s) = m {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if strings.len() == members.len() && !strings.is_empty() {
                return Some(strings);
            }
        }
        None
    }

    /// Return a display wrapper that formats type variables as readable letters
    /// (T, U, V, ...) instead of internal IDs. Used for stdlib hover/completion display.
    pub fn display_for_stdlib(&self) -> TypeDisplay<'_> {
        TypeDisplay {
            ty: self,
            style: TypeDisplayStyle::Stdlib,
        }
    }
}

// ── Type Display ────────────────────────────────────────────────

/// Controls how types are formatted as strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeDisplayStyle {
    /// Default style used in error messages and LSP hover.
    /// Type vars shown as `?T0`, tuples as `(a, b)`, optional params shown with `= None`.
    Default,
    /// Stdlib style used in stdlib hover/completion display.
    /// Type vars shown as letter names (`T`, `U`, `V`), tuples as `[a, b]`.
    Stdlib,
}

/// A wrapper for displaying a `Type` with a specific style.
pub struct TypeDisplay<'a> {
    ty: &'a Type,
    style: TypeDisplayStyle,
}

/// Pretty-print a type variable index as a letter (0 -> T, 1 -> U, 2 -> V, ...).
fn type_var_letter(index: usize) -> &'static str {
    match index {
        0 => "T",
        1 => "U",
        2 => "V",
        3 => "W",
        _ => "T",
    }
}

/// Core type formatting logic shared by all display styles.
fn fmt_type(ty: &Type, style: TypeDisplayStyle, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match ty {
        Type::Number => f.write_str("number"),
        Type::String => f.write_str("string"),
        Type::Bool => f.write_str("boolean"),
        Type::Undefined => f.write_str("undefined"),
        Type::Named(n) | Type::Foreign(n) => f.write_str(n),
        Type::Promise(inner) => {
            write!(f, "Promise<{}>", TypeDisplay { ty: inner, style })
        }
        Type::Opaque { name, .. } => f.write_str(name),
        Type::Settable(inner) => {
            write!(f, "Settable<{}>", TypeDisplay { ty: inner, style })
        }
        Type::Function {
            params,
            required_params,
            return_type,
        } => {
            f.write_str("(")?;
            for (i, t) in params.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{}", TypeDisplay { ty: t, style })?;
                if style == TypeDisplayStyle::Default
                    && i >= *required_params
                    && *required_params < params.len()
                {
                    f.write_str(" = None")?;
                }
            }
            write!(
                f,
                ") -> {}",
                TypeDisplay {
                    ty: return_type,
                    style
                }
            )
        }
        Type::Array(inner) => {
            write!(f, "Array<{}>", TypeDisplay { ty: inner, style })
        }
        Type::Map { key, value } => {
            write!(
                f,
                "Map<{}, {}>",
                TypeDisplay { ty: key, style },
                TypeDisplay { ty: value, style }
            )
        }
        Type::RecordMap { key, value } => {
            write!(
                f,
                "Record<{}, {}>",
                TypeDisplay { ty: key, style },
                TypeDisplay { ty: value, style }
            )
        }
        Type::Set { element } => {
            write!(f, "Set<{}>", TypeDisplay { ty: element, style })
        }
        Type::Tuple(types) => {
            let (open, close) = match style {
                TypeDisplayStyle::Stdlib => ("[", "]"),
                TypeDisplayStyle::Default => ("(", ")"),
            };
            f.write_str(open)?;
            for (i, t) in types.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{}", TypeDisplay { ty: t, style })?;
            }
            f.write_str(close)
        }
        Type::Record(fields) => {
            f.write_str("{ ")?;
            for (i, (n, t)) in fields.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{n}: {}", TypeDisplay { ty: t, style })?;
            }
            f.write_str(" }")
        }
        Type::Union { name, .. } => {
            if name == crate::type_layout::TYPE_OPTION
                && let Some(inner) = ty.option_inner()
            {
                return write!(f, "Option<{}>", TypeDisplay { ty: inner, style });
            }
            if name == crate::type_layout::TYPE_RESULT {
                let ok_display = ty
                    .result_ok()
                    .map(|t| format!("{}", TypeDisplay { ty: t, style }))
                    .unwrap_or_else(|| "unknown".to_string());
                let err_display = ty
                    .result_err()
                    .map(|t| format!("{}", TypeDisplay { ty: t, style }))
                    .unwrap_or_else(|| "unknown".to_string());
                return write!(f, "Result<{ok_display}, {err_display}>");
            }
            f.write_str(name)
        }
        Type::TsUnion(members) => {
            for (i, t) in members.iter().enumerate() {
                if i > 0 {
                    f.write_str(" | ")?;
                }
                write!(f, "{}", TypeDisplay { ty: t, style })?;
            }
            Ok(())
        }
        Type::StringLiteral(s) => write!(f, "\"{s}\""),
        Type::Var(id) => match style {
            TypeDisplayStyle::Stdlib => f.write_str(type_var_letter(*id)),
            TypeDisplayStyle::Default => write!(f, "?T{id}"),
        },
        Type::Unknown => f.write_str("unknown"),
        Type::Error => f.write_str("<error>"),
        Type::Unit => f.write_str("()"),
        Type::Never => f.write_str("never"),
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_type(self, TypeDisplayStyle::Default, f)
    }
}

impl fmt::Display for TypeDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_type(self.ty, self.style, f)
    }
}

// ── Type Environment ─────────────────────────────────────────────

/// Tracks types of variables, functions, and type declarations in scope.
#[derive(Debug, Clone)]
pub(crate) struct TypeEnv {
    /// Stack of scopes (innermost last). Each scope maps names to types.
    pub(crate) scopes: Vec<HashMap<String, Type>>,
    /// Type declarations: type name -> TypeDef + metadata
    type_defs: HashMap<String, TypeInfo>,
    /// Trait bounds on type parameters: param name -> [trait names]
    type_param_bounds: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct TypeInfo {
    #[allow(dead_code)]
    pub(crate) def: TypeDef,
    pub(crate) opaque: bool,
    #[allow(dead_code)]
    pub(crate) type_params: Vec<String>,
}

impl TypeEnv {
    pub(crate) fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            type_defs: HashMap::new(),
            type_param_bounds: HashMap::new(),
        }
    }

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub(crate) fn define(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    /// Define a name in the parent scope (second-to-last), used to update
    /// function types after inferring the return type from the body.
    pub(crate) fn define_in_parent_scope(&mut self, name: &str, ty: Type) {
        let len = self.scopes.len();
        if len >= 2 {
            self.scopes[len - 2].insert(name.to_string(), ty);
        }
    }

    /// Check if a name is defined in the current (innermost) scope only.
    pub(crate) fn is_defined_in_current_scope(&self, name: &str) -> bool {
        self.scopes
            .last()
            .is_some_and(|scope| scope.contains_key(name))
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    pub(crate) fn define_type(&mut self, name: &str, info: TypeInfo) {
        self.type_defs.insert(name.to_string(), info);
    }

    pub(crate) fn lookup_type(&self, name: &str) -> Option<&TypeInfo> {
        self.type_defs.get(name)
    }

    /// Define trait bounds for a type parameter.
    pub(crate) fn define_type_param_bounds(&mut self, name: &str, bounds: Vec<String>) {
        self.type_param_bounds.insert(name.to_string(), bounds);
    }

    /// Resolve a `Type::Named("Foo")` to its concrete type by looking up the type definition.
    /// For records, returns `Type::Record(fields)`. For unions, returns `Type::Union`.
    /// For aliases, follows the chain. Primitives and non-Named types pass through unchanged.
    pub(crate) fn resolve_to_concrete(
        &self,
        ty: &Type,
        resolve_type_fn: &dyn Fn(&crate::parser::ast::TypeExpr) -> Type,
    ) -> Type {
        match ty {
            Type::Named(name) | Type::Foreign(name) => {
                if let Some(info) = self.lookup_type(name) {
                    self.resolve_type_def(name, info, resolve_type_fn)
                } else {
                    ty.clone()
                }
            }
            Type::Promise(_) => ty.clone(),
            _ => ty.clone(),
        }
    }

    fn resolve_type_def(
        &self,
        name: &str,
        info: &TypeInfo,
        resolve_type_fn: &dyn Fn(&crate::parser::ast::TypeExpr) -> Type,
    ) -> Type {
        match &info.def {
            crate::parser::ast::TypeDef::Record(entries) => {
                let field_types: Vec<_> = entries
                    .iter()
                    .filter_map(|e| e.as_field())
                    .map(|f| (f.name.clone(), resolve_type_fn(&f.type_ann)))
                    .collect();
                Type::Record(field_types)
            }
            crate::parser::ast::TypeDef::Union(variants) => {
                let var_types: Vec<_> = variants
                    .iter()
                    .map(|v| {
                        let field_types: Vec<_> = v
                            .fields
                            .iter()
                            .map(|f| resolve_type_fn(&f.type_ann))
                            .collect();
                        (v.name.clone(), field_types)
                    })
                    .collect();
                Type::Union {
                    name: name.to_string(),
                    variants: var_types,
                }
            }
            crate::parser::ast::TypeDef::StringLiteralUnion(variants) => Type::TsUnion(
                variants
                    .iter()
                    .map(|s| Type::StringLiteral(s.clone()))
                    .collect(),
            ),
            crate::parser::ast::TypeDef::Alias(type_expr) => {
                // For typeof aliases, use the pre-resolved env binding
                // (simple_resolve_type_expr can't resolve typeof without env context)
                if matches!(type_expr.kind, crate::parser::ast::TypeExprKind::TypeOf(_))
                    && let Some(resolved) = self.lookup(name)
                {
                    return self.resolve_to_concrete(&resolved.clone(), resolve_type_fn);
                }
                let resolved = resolve_type_fn(type_expr);
                self.resolve_to_concrete(&resolved, resolve_type_fn)
            }
        }
    }
}
