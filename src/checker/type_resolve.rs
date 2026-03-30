use super::*;

impl Checker {
    pub(crate) fn resolve_type(&mut self, type_expr: &TypeExpr) -> Type {
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
                    self.emit_error_with_help(
                        format!("cannot use `typeof` on undefined binding `{name}`"),
                        type_expr.span,
                        "E002",
                        "not defined",
                        "typeof can only be used with value bindings (const, fn)",
                    );
                    Type::Unknown
                }
            }
        }
    }

    pub(crate) fn resolve_named_type(
        &mut self,
        name: &str,
        type_args: &[TypeExpr],
        span: Span,
    ) -> Type {
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
                    self.emit_error_with_help(
                        format!("unknown type `{name}`"),
                        span,
                        "E002",
                        "not defined",
                        "check the spelling or import/define this type",
                    );
                    Type::Unknown
                }
            }
        }
    }
}
