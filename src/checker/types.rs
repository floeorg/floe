use std::collections::HashMap;

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
    /// Promise<T> — async return type, unwrapped by `await`
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
    /// Function type
    Function {
        params: Vec<Type>,
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
    /// String literal union: `"GET" | "POST" | "PUT" | "DELETE"`
    StringLiteralUnion {
        name: String,
        variants: Vec<String>,
    },
    /// Untagged union from TypeScript interop: `Date | number | string`
    /// Compatible if the value matches any member type.
    TsUnion(Vec<Type>),
    /// Type variable (for inference)
    Var(usize),
    /// The unknown/any escape hatch
    Unknown,
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

    pub(crate) fn display_name(&self) -> String {
        match self {
            Type::Number => "number".to_string(),
            Type::String => "string".to_string(),
            Type::Bool => "boolean".to_string(),
            Type::Undefined => "undefined".to_string(),
            Type::Named(n) | Type::Foreign(n) => n.clone(),
            Type::Promise(inner) => format!("Promise<{}>", inner.display_name()),
            Type::Opaque { name, .. } => name.clone(),
            Type::Settable(inner) => format!("Settable<{}>", inner.display_name()),
            Type::Function {
                params,
                return_type,
            } => {
                let p: Vec<_> = params.iter().map(|t| t.display_name()).collect();
                format!("({}) -> {}", p.join(", "), return_type.display_name())
            }
            Type::Array(inner) => format!("Array<{}>", inner.display_name()),
            Type::Map { key, value } | Type::RecordMap { key, value } => {
                format!("Map<{}, {}>", key.display_name(), value.display_name())
            }
            Type::Set { element } => format!("Set<{}>", element.display_name()),
            Type::Tuple(types) => {
                let t: Vec<_> = types.iter().map(|t| t.display_name()).collect();
                format!("({})", t.join(", "))
            }
            Type::Record(fields) => {
                let f: Vec<_> = fields
                    .iter()
                    .map(|(n, t)| format!("{n}: {}", t.display_name()))
                    .collect();
                format!("{{ {} }}", f.join(", "))
            }
            Type::Union { name, .. } => {
                if name == crate::type_layout::TYPE_OPTION
                    && let Some(inner) = self.option_inner()
                {
                    return format!("Option<{}>", inner.display_name());
                }
                if name == crate::type_layout::TYPE_RESULT {
                    let ok = self
                        .result_ok()
                        .map(|t| t.display_name())
                        .unwrap_or_else(|| "unknown".to_string());
                    let err = self
                        .result_err()
                        .map(|t| t.display_name())
                        .unwrap_or_else(|| "unknown".to_string());
                    return format!("Result<{ok}, {err}>");
                }
                name.clone()
            }
            Type::StringLiteralUnion { name, .. } => name.clone(),
            Type::TsUnion(members) => {
                let m: Vec<_> = members.iter().map(|t| t.display_name()).collect();
                m.join(" | ")
            }
            Type::Var(id) => format!("?T{id}"),
            Type::Unknown => "unknown".to_string(),
            Type::Unit => "()".to_string(),
            Type::Never => "never".to_string(),
        }
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
            Type::Named(name) => {
                if let Some(info) = self.lookup_type(name) {
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
                                name: name.clone(),
                                variants: var_types,
                            }
                        }
                        crate::parser::ast::TypeDef::StringLiteralUnion(variants) => {
                            Type::StringLiteralUnion {
                                name: name.clone(),
                                variants: variants.clone(),
                            }
                        }
                        crate::parser::ast::TypeDef::Alias(type_expr) => {
                            // For typeof aliases, use the pre-resolved env binding
                            // (simple_resolve_type_expr can't resolve typeof without env context)
                            if matches!(type_expr.kind, crate::parser::ast::TypeExprKind::TypeOf(_))
                                && let Some(resolved) = self.lookup(name)
                            {
                                // Continue resolving in case typeof gives us another Named type
                                return self
                                    .resolve_to_concrete(&resolved.clone(), resolve_type_fn);
                            }
                            let resolved = resolve_type_fn(type_expr);
                            // Follow alias chains
                            self.resolve_to_concrete(&resolved, resolve_type_fn)
                        }
                    }
                } else {
                    ty.clone()
                }
            }
            Type::Foreign(_) | Type::Promise(_) => ty.clone(),
            _ => ty.clone(),
        }
    }
}
