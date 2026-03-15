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
    /// Unit type () — replaces void, a real value usable in generics
    Unit,
}

impl Type {
    pub(crate) fn is_result(&self) -> bool {
        matches!(self, Type::Result { .. })
    }

    pub(crate) fn is_option(&self) -> bool {
        matches!(self, Type::Option(_))
    }

    pub(crate) fn is_numeric(&self) -> bool {
        matches!(self, Type::Number)
    }

    pub(crate) fn display_name(&self) -> String {
        match self {
            Type::Number => "number".to_string(),
            Type::String => "string".to_string(),
            Type::Bool => "boolean".to_string(),
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
                format!("({}) -> {}", p.join(", "), return_type.display_name())
            }
            Type::Array(inner) => format!("Array<{}>", inner.display_name()),
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
            Type::Union { name, .. } => name.clone(),
            Type::Var(id) => format!("?T{id}"),
            Type::Unknown => "unknown".to_string(),
            Type::Unit => "()".to_string(),
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

    /// Check if a name is already defined in any scope.
    pub(crate) fn is_defined_in_any_scope(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains_key(name))
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
                        crate::parser::ast::TypeDef::Record(fields) => {
                            let field_types: Vec<_> = fields
                                .iter()
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
                        crate::parser::ast::TypeDef::Alias(type_expr) => {
                            let resolved = resolve_type_fn(type_expr);
                            // Follow alias chains
                            self.resolve_to_concrete(&resolved, resolve_type_fn)
                        }
                    }
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }
}
