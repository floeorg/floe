//! Boundary type wrapping: converts TypeScript types to Floe types at the import boundary.

use super::*;

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

        TsType::Named(name) => Type::Named(name.clone()),

        TsType::Generic { name, args } => {
            match name.as_str() {
                "Promise" if args.len() == 1 => {
                    // Promise<T> stays as a named type
                    Type::Named(format!("Promise<{:?}>", wrap_boundary_type(&args[0])))
                }
                _ => {
                    // Generic named type
                    Type::Named(name.clone())
                }
            }
        }

        TsType::Union(parts) => wrap_union_boundary(parts),

        TsType::Function {
            params,
            return_type,
        } => {
            let wrapped_params: Vec<Type> = params.iter().map(wrap_boundary_type).collect();
            let wrapped_return = wrap_boundary_type(return_type);
            Type::Function {
                params: wrapped_params,
                return_type: Box::new(wrapped_return),
            }
        }

        TsType::Array(inner) => Type::Array(Box::new(wrap_boundary_type(inner))),

        TsType::Object(fields) => {
            let wrapped: Vec<(String, Type)> = fields
                .iter()
                .map(|(name, ty)| (name.clone(), wrap_boundary_type(ty)))
                .collect();
            Type::Record(wrapped)
        }

        TsType::Tuple(parts) => Type::Tuple(parts.iter().map(wrap_boundary_type).collect()),
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

    let inner_type = if non_null_parts.len() == 1 {
        wrap_boundary_type(non_null_parts[0])
    } else if non_null_parts.is_empty() {
        // `null | undefined` -> Option<Void> (shouldn't happen in practice)
        Type::Unit
    } else {
        // Multi-type union without null/undefined: stay as Unknown for now
        // A full implementation would create proper union types
        Type::Unknown
    };

    if nullable {
        Type::Option(Box::new(inner_type))
    } else {
        inner_type
    }
}
