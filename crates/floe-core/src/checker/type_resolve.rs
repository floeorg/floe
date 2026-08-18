use std::sync::Arc;

use super::{Checker, ErrorCode, Span, Type, TypeExpr, TypeExprKind, expr, type_layout, type_var};

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
                let param_types: Vec<_> = params
                    .iter()
                    .map(|p| self.resolve_type(&p.type_ann))
                    .collect();
                let ret = self.resolve_type(return_type);
                let required_params = param_types.len();
                Type::Function {
                    params: param_types,
                    return_type: Arc::new(ret),
                    required_params,
                }
            }
            TypeExprKind::Array(inner) => Type::Array(Arc::new(self.resolve_type(inner))),
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
            TypeExprKind::StringLiteral(value) => Type::foreign(format!("\"{value}\"")),
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
                        ErrorCode::UndefinedName,
                        "not defined",
                        "typeof can only be used with value bindings (const, fn)",
                    );
                    Type::Error
                }
            }
        }
    }

    fn check_type_arg_arity(&mut self, name: &str, expected: usize, actual: usize, span: Span) {
        // Skip when no type args are provided — bare `Option`, `Result`, etc. are
        // valid (inner types default to Unknown and may be inferred later).
        if actual == expected || actual == 0 {
            return;
        }
        // A stdlib type such as `URL` takes no arguments at all, so it needs
        // its own wording. "expects 0 type argument(s)" reads as a bug.
        let (message, help) = if expected == 0 {
            (
                format!("`{name}` takes no type arguments, found {actual}"),
                format!("`{name}` is not a generic type"),
            )
        } else {
            (
                format!("`{name}` expects {expected} type argument(s), found {actual}"),
                format!("`{name}` takes exactly {expected} type argument(s)"),
            )
        };

        self.emit_error_with_help(
            message,
            span,
            ErrorCode::TypeArgumentArity,
            "wrong number of type arguments",
            help,
        );
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn resolve_named_type(
        &mut self,
        name: &str,
        type_args: &[TypeExpr],
        span: Span,
    ) -> Type {
        // Mark type names as used (e.g. "JSX" from "JSX.Element", or "User")
        let root = name.split('.').next().unwrap_or(name);
        self.unused.used_names.insert(root.to_string());

        // A user-declared generic type parameter (`T`, `U`, …) resolves to
        // its hydrated `Generic` variable — each occurrence of the same name
        // inside the signature gets the same Generic, so inference sees them
        // as tied together.
        if type_args.is_empty()
            && let Some(g) = self.active_type_params.get(name)
        {
            return g.clone();
        }

        match name {
            type_layout::TYPE_NUMBER => Type::Number,
            type_layout::TYPE_STRING => Type::String,
            type_layout::TYPE_BOOLEAN => Type::Bool,
            type_layout::TYPE_UNIT => Type::Unit,
            type_layout::TYPE_UNDEFINED => Type::Undefined,
            type_layout::TYPE_UNKNOWN => Type::Unknown,
            type_layout::TYPE_ERROR | type_layout::TYPE_RESPONSE => Type::Named(name.to_string()),
            type_layout::TYPE_RESULT => {
                self.check_type_arg_arity(name, 2, type_args.len(), span);
                let ok = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                let err = type_args
                    .get(1)
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::result_of(ok, err)
            }
            type_layout::TYPE_OPTION => {
                self.check_type_arg_arity(name, 1, type_args.len(), span);
                let inner = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::option_of(inner)
            }
            type_layout::TYPE_SETTABLE => {
                self.check_type_arg_arity(name, 1, type_args.len(), span);
                let inner = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::Settable(Arc::new(inner))
            }
            type_layout::TYPE_ARRAY => {
                self.check_type_arg_arity(name, 1, type_args.len(), span);
                let inner = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::Array(Arc::new(inner))
            }
            type_layout::TYPE_PROMISE => {
                self.check_type_arg_arity(name, 1, type_args.len(), span);
                let inner = type_args
                    .first()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or(Type::Unknown);
                Type::Promise(Arc::new(inner))
            }
            _ => {
                // Trait names are not types — always error when used in a type position.
                if !self.registering_types && self.traits.trait_defs.contains_key(name) {
                    self.emit_error_with_help(
                        format!("`{name}` is a trait, not a type — traits cannot be used in type positions"),
                        span,
                        ErrorCode::TraitUsedAsType,
                        "trait, not a type",
                        "traits are compile-time contracts and cannot appear as types",
                    );
                    return Type::Error;
                }

                // Check if this is a known user-defined type or imported name.
                // Skip validation during type registration (forward references).
                // If the env has a Foreign type, preserve it — and encode any
                // concrete type arguments into the name so `Router<Bindings>`
                // doesn't collapse to `Router` (which would make `Router<A>`
                // and `Router<B>` indistinguishable). Encoding is skipped when
                // any argument is a generic type parameter because Type::Generic
                // isn't substituted through Foreign name strings: baking "E"
                // into a Foreign name would leak the unresolved placeholder
                // past instantiation sites.
                if let Some(Type::Foreign { .. }) = self.env.lookup(name) {
                    let mut resolved_args: Vec<Type> =
                        type_args.iter().map(|t| self.resolve_type(t)).collect();
                    // Pad with .d.ts-declared defaults when the user supplied
                    // fewer args than the generic has params. Stops at the
                    // first param with no default, matching TypeScript's own
                    // behavior (defaults must form a contiguous tail).
                    self.pad_with_dts_defaults(name, &mut resolved_args);
                    if resolved_args.is_empty() {
                        Type::foreign(name.to_string())
                    } else {
                        let any_generic = resolved_args
                            .iter()
                            .any(|t| self.type_contains_active_param(t));
                        if any_generic {
                            Type::foreign(name.to_string())
                        } else {
                            let args_str: Vec<String> =
                                resolved_args.iter().map(|t| t.to_string()).collect();
                            Type::foreign(format!("{}<{}>", name, args_str.join(", ")))
                        }
                    }
                } else if self.registering_types || self.env.lookup_type(name).is_some() {
                    Type::Named(name.to_string())
                } else if name.contains('.') {
                    self.resolve_dotted_type(name, span)
                } else if let Some(ty) = self.env.lookup(name) {
                    // Accept values as type names if they represent type-like values:
                    // unions (variant constructors), records (imported TS objects),
                    // or named types. Reject arbitrary values like component functions.
                    if matches!(ty, Type::Union { .. } | Type::Record(_) | Type::Named(_)) {
                        Type::Named(name.to_string())
                    } else if matches!(ty, Type::Function { .. }) && self.is_imported_name(name) {
                        // Imported function-type aliases (hono's `type Next =
                        // () => Promise<void>`) are usable in type position;
                        // locally-declared Floe fns are not (hence the
                        // import-only guard).
                        ty.clone()
                    } else {
                        self.emit_error_with_help(
                            format!("`{name}` is a value, not a type"),
                            span,
                            ErrorCode::UndefinedName,
                            "cannot use a value as a type",
                            "check the spelling or import/define this type",
                        );
                        Type::Error
                    }
                } else if self.ambient_types.contains_key(name) {
                    // Accept ambient type names from TypeScript lib definitions
                    // (e.g., Date, RegExp, URL, HTMLElement) as valid type annotations.
                    Type::Named(name.to_string())
                } else if self.stdlib.declares_type(name) && !self.is_imported_name(name) {
                    // A type the stdlib itself declares, such as `URL`.
                    // TypeScript cannot teach the resolver these names when
                    // the project tsconfig omits the lib that holds them, so
                    // the stdlib registry answers instead.
                    //
                    // Everything the user wrote is checked first. A value
                    // binding of the same name reaches the branch above and
                    // keeps its error, and an import keeps its own type, which
                    // is what the `is_imported_name` guard covers for a name
                    // that never reached the env. `register_ambient_types`
                    // leaves the name unbound, so an ambient constructor value
                    // does not shadow the type.
                    self.check_type_arg_arity(name, 0, type_args.len(), span);
                    // Resolve the arguments even though the type takes none.
                    // The walk marks every name inside them used, and reports
                    // a name that does not resolve.
                    for arg in type_args {
                        self.resolve_type(arg);
                    }
                    Type::Named(name.to_string())
                } else if type_layout::is_ts_utility_type(name) {
                    // Resolve args so inner references are marked used; TS resolves
                    // the utility-type semantics at its own compile time.
                    for arg in type_args {
                        self.resolve_type(arg);
                    }
                    Type::Named(name.to_string())
                } else {
                    self.emit_error_with_help(
                        format!("unknown type `{name}`"),
                        span,
                        ErrorCode::UndefinedName,
                        "not defined",
                        "check the spelling or import/define this type",
                    );
                    Type::Error
                }
            }
        }
    }

    /// Resolve a dotted type name such as `JSX.Element` or `Intl.DateTimeFormat`.
    ///
    /// Floe does not model TypeScript namespaces yet (#848), so the resolver
    /// cannot walk a dotted name segment by segment. It classifies the name
    /// against what the loaders recorded, and reports E002 unless a loader
    /// declared the whole name. See `classify_dotted_type`.
    fn resolve_dotted_type(&mut self, name: &str, span: Span) -> Type {
        let help = match self.classify_dotted_type(name) {
            DottedType::Declared | DottedType::Unlisted => return Type::Named(name.to_string()),
            DottedType::UnknownMember { namespace, member } => format!(
                "`{namespace}` declares no type `{member}`; check the spelling, or the namespace this type comes from"
            ),
            DottedType::UnknownRoot { root } => format!(
                "`{root}` is not an imported name and not an ambient namespace, so check the spelling, or import the namespace this type comes from"
            ),
        };

        self.emit_error_with_help(
            format!("unknown type `{name}`"),
            span,
            ErrorCode::UndefinedName,
            "not defined",
            help,
        );

        Type::Error
    }

    /// Classify a dotted type name against the namespaces the loaders
    /// described.
    ///
    /// A loader records a namespace member twice: under its bare name, and
    /// under its qualified name. So a name the tables carry is declared, and
    /// a name they do not carry is a bad member of a namespace they do
    /// describe. A namespace that no loader described at all leaves the
    /// member unchecked, because there is no member list to test it against.
    ///
    /// `JSX` is unchecked on purpose. See the arm that names it.
    ///
    /// The first segment is deliberately not looked up as a value, as a Floe
    /// type, or as an ambient interface. None of the three has members, so
    /// `Option.Option.Aaa` and `Element.Anything` are both typos, and neither
    /// must resolve.
    fn classify_dotted_type<'a>(&self, name: &'a str) -> DottedType<'a> {
        if self.ambient_types.contains_key(name)
            || type_layout::is_builtin_type(name)
            || self.dts_imports_declare(name)
        {
            return DottedType::Declared;
        }

        let root = name.split('.').next().unwrap_or(name);

        // `JSX` is unchecked, and this arm is deliberate. Floe holds one
        // hardcoded name for the namespace it owns, `JSX.Element` in
        // `type_layout`, and one name is not a member list. `JSX` is
        // therefore a namespace whose contents Floe cannot describe, so the
        // member stays unchecked, the same as an imported root that no
        // loader listed.
        //
        // TypeScript does describe it. An emitted component names React's
        // `JSX` namespace, so `JSX.IntrinsicElements` type checks there. A
        // checker that rejected the name would disagree with the code it
        // emits, which is worse than not checking it. #1544 gives `JSX` a
        // real member list, and this arm moves to the member check then.
        if type_layout::is_builtin_namespace(root) {
            return DottedType::Unlisted;
        }

        // An ambient namespace always carries its member list, because one
        // walk records the namespace and its members together.
        if let Some(namespace) = self.ambient_namespace_prefix(name) {
            return DottedType::unknown_member(name, namespace);
        }

        if self.imported_root_names.contains(root) {
            if self.dts_imports_list_members_of(root) {
                return DottedType::unknown_member(name, root);
            }

            return DottedType::Unlisted;
        }

        DottedType::UnknownRoot { root }
    }

    /// True when an imported module declares `name` as a namespace member.
    fn dts_imports_declare(&self, name: &str) -> bool {
        self.dts_imports
            .values()
            .any(|exports| exports.iter().any(|export| export.name == name))
    }

    /// True when an imported module lists any member of the namespace
    /// `root`, so a member of `root` that is absent is a typo rather than a
    /// name the loader never saw.
    fn dts_imports_list_members_of(&self, root: &str) -> bool {
        let prefix = format!("{root}.");

        self.dts_imports.values().any(|exports| {
            exports
                .iter()
                .any(|export| export.name.starts_with(&prefix))
        })
    }

    /// The longest dotted prefix of `name` that is an ambient namespace.
    ///
    /// The ambient loader records a nested namespace under its qualified
    /// name, so `@types/react` contributes `React` and `React.JSX`. This
    /// tests the longest prefix first, so `React.JSX.Element` matches the
    /// entry `React.JSX` rather than the bare root `React`.
    fn ambient_namespace_prefix<'a>(&self, name: &'a str) -> Option<&'a str> {
        let mut prefix_ends: Vec<usize> = name.match_indices('.').map(|(at, _)| at).collect();

        while let Some(end) = prefix_ends.pop() {
            if self.ambient_namespaces.contains(&name[..end]) {
                return Some(&name[..end]);
            }
        }

        None
    }

    /// True if `ty` contains anywhere in its tree an unresolved type-parameter
    /// marker: a fresh inference `Var`, or a `Named` whose name is a currently-
    /// active user-written generic (e.g. `E` inside `Context<{ Bindings: E }>`).
    /// Nested params must trigger the same bare-Foreign fallback as top-level
    /// ones — otherwise the placeholder leaks into the encoded Foreign name
    /// string and the Foreign-vs-Foreign compat check rejects legitimate
    /// instantiations.
    fn type_contains_active_param(&self, ty: &Type) -> bool {
        type_var::any_nested(ty, &|t: &Type| match t {
            Type::Var(_) => true,
            Type::Named(n) => self.active_type_params.contains_key(n.as_str()),
            _ => false,
        })
    }

    /// Pad `args` with .d.ts-declared default type parameters for generic
    /// `name`, so a user-written `Foo<A>` materializes as `Foo<A, DefB>`
    /// when the declaration reads `Foo<A, B = DefB>`. Padding stops at the
    /// first param with no default; TypeScript requires defaults to form a
    /// contiguous tail, and we mirror that.
    fn pad_with_dts_defaults(&self, name: &str, args: &mut Vec<Type>) {
        pad_foreign_args_with_defaults(&self.dts_generic_params, name, args);
    }
}

/// Module-level helper so `wrap_boundary_type` callers can apply the same
/// padding without going through a `Checker` method. `args` is mutated in
/// place — padding stops at the first parameter with no default.
pub(crate) fn pad_foreign_args_with_defaults(
    registry: &std::collections::HashMap<String, Vec<crate::interop::GenericParamInfo>>,
    name: &str,
    args: &mut Vec<Type>,
) {
    let Some(param_infos) = registry.get(name) else {
        return;
    };
    if args.len() >= param_infos.len() {
        return;
    }
    for info in &param_infos[args.len()..] {
        let Some(default_ts) = &info.default else {
            break;
        };
        args.push(crate::interop::wrap_boundary_type(default_ts));
    }
}

/// How a dotted type name stands against the namespaces the interop loaders
/// described.
enum DottedType<'a> {
    /// A loader recorded the whole name, so the name is a type.
    Declared,
    /// A loader described the namespace, and the namespace holds no such
    /// type.
    UnknownMember { namespace: &'a str, member: &'a str },
    /// The first segment names something real, and no loader listed what
    /// that thing holds. The member stays unchecked.
    Unlisted,
    /// Nothing knows the first segment.
    UnknownRoot { root: &'a str },
}

impl<'a> DottedType<'a> {
    /// Build the unknown-member answer for `name` under `namespace`.
    ///
    /// `namespace` is a prefix of `name`, so the member is the rest of the
    /// name after the dot that follows the prefix.
    fn unknown_member(name: &'a str, namespace: &'a str) -> Self {
        DottedType::UnknownMember {
            namespace,
            member: &name[namespace.len() + 1..],
        }
    }
}
