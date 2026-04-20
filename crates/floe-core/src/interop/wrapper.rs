//! Boundary type wrapping: converts TypeScript types to Floe types at the import boundary.
use std::sync::Arc;

use super::*;
use crate::type_layout;

/// Converts a TypeScript type to a Floe type, applying boundary wrapping:
/// - `T | null` -> `Option<T>`
/// - `T | undefined` -> `Option<T>`
/// - `T | null | undefined` -> `Option<T>`
/// - `any` -> `unknown`
pub fn wrap_boundary_type(ts_type: &TsType) -> Type {
    match ts_type {
        TsType::Primitive(name) => match name.as_str() {
            "string" => Type::String,
            "number" => Type::Number,
            "boolean" => Type::Bool,
            "void" => Type::Unit,
            "never" => Type::Unit,
            _ => Type::Unknown,
        },

        TsType::Null | TsType::Undefined => Type::Undefined,

        // any -> unknown (forces narrowing in Floe)
        TsType::Any => Type::Unknown,

        TsType::Unknown => Type::Unknown,

        TsType::Named(name) => {
            // Single uppercase letter = generic type variable (T, U, S)
            if name.len() == 1 && name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                Type::Named(name.clone())
            } else {
                Type::foreign(name.clone())
            }
        }

        TsType::Generic { name, args } => {
            match name.as_str() {
                "Array" | "ReadonlyArray" if args.len() == 1 => {
                    Type::Array(Arc::new(wrap_boundary_type(&args[0])))
                }
                "Promise" if args.len() == 1 => {
                    Type::Promise(Arc::new(wrap_boundary_type(&args[0])))
                }
                // FloeOption<T> → Option<T> (our probe wrapper for Option)
                "FloeOption" if args.len() == 1 => Type::option_of(wrap_boundary_type(&args[0])),
                // TS Record<K, V> → Floe RecordMap<K, V> (plain-object map)
                "Record" if args.len() == 2 => Type::RecordMap {
                    key: Arc::new(wrap_boundary_type(&args[0])),
                    value: Arc::new(wrap_boundary_type(&args[1])),
                },
                // React's Dispatch<SetStateAction<T>> is a function: (T) -> ()
                "Dispatch" if args.len() == 1 => {
                    let inner = unwrap_set_state_action(&args[0]);
                    Type::Function {
                        params: vec![wrap_boundary_type(inner)],
                        return_type: Arc::new(Type::Unit),
                        required_params: 1,
                    }
                }
                _ => {
                    // If any arg contains a type-parameter placeholder (a
                    // single-uppercase-letter `Named`, at any nesting depth —
                    // e.g. `E` inside `Context<{ Bindings: E }>`), the
                    // placeholder would leak into the encoded name and block
                    // later substitution. Fall back to the bare name so the
                    // Foreign-vs-Foreign compat check can stay permissive
                    // for unresolved generics (mirrors the same guard in
                    // `resolve_named_type`).
                    let wrapped_args: Vec<Type> = args.iter().map(wrap_boundary_type).collect();
                    if wrapped_args.iter().any(contains_placeholder_param) {
                        return Type::foreign(name.clone());
                    }
                    let args_str: Vec<String> =
                        wrapped_args.iter().map(|a| a.to_string()).collect();
                    Type::foreign(format!("{}<{}>", name, args_str.join(", ")))
                }
            }
        }

        TsType::Union(parts) => wrap_union_boundary(parts),

        TsType::Function {
            params,
            return_type,
        } => {
            let required_params = params.iter().filter(|p| !p.optional).count();
            let wrapped_params: Vec<Type> = params
                .iter()
                .map(|p| {
                    let ty = wrap_boundary_type(&p.ty);
                    if p.optional { Type::option_of(ty) } else { ty }
                })
                .collect();
            let wrapped_return = wrap_boundary_type(return_type);
            Type::Function {
                params: wrapped_params,
                return_type: Arc::new(wrapped_return),
                required_params,
            }
        }

        TsType::Array(inner) => Type::Array(Arc::new(wrap_boundary_type(inner))),

        TsType::Object(fields) => {
            let wrapped: Vec<(String, Type)> = fields
                .iter()
                .map(|f| {
                    let ty = if f.optional && f.ty.is_nullable() {
                        // x?: T | null → Settable<T>
                        // Wrap the non-null inner type directly, skipping the Option wrapper
                        let inner = wrap_non_null_inner(&f.ty);
                        Type::Settable(Arc::new(inner))
                    } else if f.optional {
                        // x?: T → Option<T>
                        Type::option_of(wrap_boundary_type(&f.ty))
                    } else {
                        wrap_boundary_type(&f.ty)
                    };
                    (f.name.clone(), ty)
                })
                .collect();
            Type::Record(wrapped)
        }

        TsType::Tuple(parts) => Type::Tuple(parts.iter().map(wrap_boundary_type).collect()),

        // String/number/boolean literal types carry discriminator information.
        // `StringLiteral` maps to Floe's `Type::StringLiteral` so union discrimination
        // (e.g. `"GET" | "POST"`) survives. Numeric/boolean literals don't have a
        // dedicated Floe variant — widen to the underlying primitive.
        TsType::StringLiteral(s) => Type::StringLiteral(s.clone()),
        TsType::NumberLiteral(_) => Type::Number,
        TsType::BooleanLiteral(_) => Type::Bool,

        // `this` return inside an unresolved context — the caller-side contextual
        // resolution should have replaced this with the enclosing interface name
        // before wrapping. If it survived, fall back to Unknown.
        TsType::This => Type::Unknown,

        // Indexed access `Obj["key"]` — evaluate the lookup when the
        // shape is concrete enough, else fall back to Unknown. Earlier
        // stages (checker-side generic substitution) should have
        // already reduced `E["Bindings"]` to `X["Bindings"]` when `E`
        // is bound to `X`; here we just finish the lookup.
        TsType::IndexedAccess { object, index } => super::evaluate_indexed_access(object, index)
            .map(|ts| wrap_boundary_type(&ts))
            .unwrap_or(Type::Unknown),
    }
}

/// Wraps a union type at the boundary, converting null/undefined members to Option.
fn wrap_union_boundary(parts: &[TsType]) -> Type {
    let has_null = parts.iter().any(|p| matches!(p, TsType::Null));
    let has_undefined = parts.iter().any(|p| matches!(p, TsType::Undefined));
    let nullable = has_null || has_undefined;

    // Filter out null and undefined from the union
    let non_null_parts: Vec<&TsType> = parts
        .iter()
        .filter(|p| !matches!(p, TsType::Null | TsType::Undefined))
        .collect();

    // Check for Result pattern: { ok: true, value: T } | { ok: false, error: E }
    if non_null_parts.len() == 2
        && let Some(result_type) = try_parse_result_union(&non_null_parts)
    {
        return if nullable {
            Type::option_of(result_type)
        } else {
            result_type
        };
    }

    let inner_type = if non_null_parts.len() == 1 {
        wrap_boundary_type(non_null_parts[0])
    } else if non_null_parts.is_empty() {
        // `null | undefined` -> Option<Void> (shouldn't happen in practice)
        Type::Unit
    } else if let Some(merged) = try_merge_object_union(&non_null_parts) {
        // All union members are objects — merge common fields for destructuring
        merged
    } else {
        // Multi-type union: preserve as TsUnion for strict type checking
        Type::TsUnion(
            non_null_parts
                .iter()
                .map(|p| wrap_boundary_type(p))
                .collect(),
        )
    };

    if nullable {
        Type::option_of(inner_type)
    } else {
        inner_type
    }
}

/// Try to merge a union of object types into a single Record with common fields.
/// Each field's type is the union of that field across all members.
/// Returns None if any member is not an object or if there are no common fields.
///
/// Example: `{ data: A, error: null } | { data: B, error: E }` → `Record { data: A|B, error: null|E }`
fn try_merge_object_union(parts: &[&TsType]) -> Option<Type> {
    use super::ts_types::ObjectField;
    use std::collections::HashMap;

    if parts.len() < 2 {
        return None;
    }

    // Check all parts are objects
    let objects: Vec<&Vec<ObjectField>> = parts
        .iter()
        .filter_map(|p| {
            if let TsType::Object(fields) = p {
                Some(fields)
            } else {
                None
            }
        })
        .collect();
    if objects.len() != parts.len() {
        return None;
    }

    // Find field names that appear in ALL members
    let first_names: Vec<&str> = objects[0].iter().map(|f| f.name.as_str()).collect();
    let common_names: Vec<&str> = first_names
        .into_iter()
        .filter(|name| {
            objects[1..]
                .iter()
                .all(|obj| obj.iter().any(|f| f.name == *name))
        })
        .collect();
    if common_names.is_empty() {
        return None;
    }

    // Build merged fields: each field's type is a union of its type across all members
    let mut merged_fields: Vec<(String, Type)> = Vec::new();
    for name in &common_names {
        let mut field_types: Vec<TsType> = Vec::new();
        let mut any_optional = false;
        for obj in &objects {
            if let Some(field) = obj.iter().find(|f| f.name == *name) {
                any_optional |= field.optional;
                field_types.push(field.ty.clone());
            }
        }
        // Deduplicate identical types
        field_types.dedup();
        let merged_ty = if field_types.len() == 1 {
            let ty = wrap_boundary_type(&field_types[0]);
            if any_optional {
                Type::option_of(ty)
            } else {
                ty
            }
        } else {
            // Create a union and wrap it
            let ty = wrap_boundary_type(&TsType::Union(field_types));
            if any_optional && !ty.is_option() {
                Type::option_of(ty)
            } else {
                ty
            }
        };
        // Collect into a hashmap to avoid duplicates from different key positions
        merged_fields.push((name.to_string(), merged_ty));
    }

    // Deduplicate by field name (shouldn't happen but just in case)
    let mut seen = HashMap::new();
    let deduped: Vec<(String, Type)> = merged_fields
        .into_iter()
        .filter(|(name, _)| seen.insert(name.clone(), ()).is_none())
        .collect();

    Some(Type::Record(deduped))
}

/// Try to detect the Result discriminated union pattern:
/// `{ ok: true, value: T } | { ok: false, error: E }` → `Result<T, E>`
fn try_parse_result_union(parts: &[&TsType]) -> Option<Type> {
    if parts.len() != 2 {
        return None;
    }

    let mut ok_type = None;
    let mut err_type = None;

    for part in parts {
        if let TsType::Object(fields) = part {
            let ok_field = fields.iter().find(|f| f.name == type_layout::OK_FIELD);
            let value_field = fields.iter().find(|f| f.name == type_layout::VALUE_FIELD);
            let error_field = fields.iter().find(|f| f.name == type_layout::ERROR_FIELD);

            if value_field.is_some() && ok_field.is_some() {
                ok_type = value_field.map(|f| wrap_boundary_type(&f.ty));
            } else if error_field.is_some() && ok_field.is_some() {
                err_type = error_field.map(|f| wrap_boundary_type(&f.ty));
            }
        }
    }

    if let (Some(ok), Some(err)) = (ok_type, err_type) {
        Some(Type::result_of(ok, err))
    } else {
        None
    }
}

/// Extract the non-null/non-undefined inner type and wrap it.
/// For `T | null` returns wrapped T. For bare `null` returns Unit.
fn wrap_non_null_inner(ty: &TsType) -> Type {
    match ty {
        TsType::Union(parts) => {
            let non_null: Vec<&TsType> = parts
                .iter()
                .filter(|p| !matches!(p, TsType::Null | TsType::Undefined))
                .collect();
            if non_null.len() == 1 {
                wrap_boundary_type(non_null[0])
            } else if non_null.is_empty() {
                Type::Unit
            } else {
                Type::Unknown
            }
        }
        TsType::Null | TsType::Undefined => Type::Unit,
        other => wrap_boundary_type(other),
    }
}

/// True if `ty` contains anywhere in its tree a single-uppercase-letter
/// `Named` — the convention for an unresolved TypeScript generic parameter
/// (`T`, `E`, `U`). Used when deciding whether to encode generic args into
/// a Foreign name string: a placeholder baked into the string blocks later
/// substitution during Foreign-vs-Foreign compat.
fn contains_placeholder_param(ty: &Type) -> bool {
    crate::checker::type_any_nested(ty, &|t: &Type| match t {
        Type::Var(_) => true,
        Type::Named(n) => n.len() == 1 && n.chars().next().is_some_and(|c| c.is_ascii_uppercase()),
        _ => false,
    })
}

/// Unwrap SetStateAction<T> → T. If not a SetStateAction, return as-is.
fn unwrap_set_state_action(ty: &TsType) -> &TsType {
    if let TsType::Generic { name, args } = ty
        && name == "SetStateAction"
        && args.len() == 1
    {
        &args[0]
    } else {
        ty
    }
}
