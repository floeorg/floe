//! Type environment: the scope stack of variable bindings, type
//! declarations, and type-parameter trait bounds that the checker
//! consults to resolve identifiers and type names.
//!
//! Extracted from the former `checker/types.rs` as part of #1107 so
//! that `types.rs` holds only the `Type` data structure and the
//! environment can evolve independently (future `Arc<RefCell<TypeVar>>`
//! unification in #1118 will plug into this module).

use std::collections::HashMap;

use crate::parser::ast::TypeDef;

use super::types::Type;

/// Tracks types of variables, functions, and type declarations in scope.
#[derive(Debug, Clone)]
pub(crate) struct TypeEnv {
    /// Stack of scopes (innermost last). Each scope maps names to types.
    pub(crate) scopes: Vec<HashMap<String, Type>>,
    /// Parallel stack of definition spans — one span per name in the
    /// matching `scopes` entry. Populated when a name is defined with a
    /// known span so the reference tracker can wire references back to
    /// their definitions.
    def_spans: Vec<HashMap<String, crate::lexer::span::Span>>,
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
            def_spans: vec![HashMap::new()],
            type_defs: HashMap::new(),
            type_param_bounds: HashMap::new(),
        }
    }

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.def_spans.push(HashMap::new());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.scopes.pop();
        self.def_spans.pop();
    }

    pub(crate) fn define(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    /// Define a name along with the span of its declaration site. LSP
    /// reference-tracking keys off these spans; definitions without a
    /// span (stdlib, synthetic helpers) use `define`.
    pub(crate) fn define_with_span(
        &mut self,
        name: &str,
        ty: Type,
        span: crate::lexer::span::Span,
    ) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
        if let Some(spans) = self.def_spans.last_mut() {
            spans.insert(name.to_string(), span);
        }
    }

    /// Look up the definition span for a name, searching outward through
    /// the scope stack. Returns `None` for names that were defined without
    /// a span or aren't defined in any live scope.
    pub(crate) fn lookup_def_span(&self, name: &str) -> Option<crate::lexer::span::Span> {
        for spans in self.def_spans.iter().rev() {
            if let Some(span) = spans.get(name) {
                return Some(*span);
            }
        }
        None
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

    /// Look up trait bounds for a type parameter.
    pub(crate) fn get_type_param_bounds(&self, name: &str) -> Option<&Vec<String>> {
        self.type_param_bounds.get(name)
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
            Type::Named(name) | Type::Foreign { name, .. } => {
                if let Some(info) = self.lookup_type(name) {
                    self.resolve_type_def(name, info, resolve_type_fn)
                } else {
                    ty.clone()
                }
            }
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
