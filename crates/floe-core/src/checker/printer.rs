//! Pretty-printing for `Type` values: the surface form users see in
//! diagnostics, LSP hover, and stdlib documentation.
//!
//! Extracted from `checker/types.rs` as part of #1107 so the Type
//! enum stays focused on the data structure and display rules can
//! evolve without touching the core type definition.

use std::fmt;

use super::types::Type;

/// Controls how types are formatted as strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeDisplayStyle {
    /// Default style used in error messages and LSP hover.
    /// Type vars shown as `?T0`, tuples as `(a, b)`, optional params shown with `= None`.
    Default,
    /// Stdlib style used in stdlib hover/completion display.
    /// Type vars shown as letter names (`T`, `U`, `V`), tuples as `[a, b]`.
    Stdlib,
}

/// A wrapper for displaying a `Type` with a specific style.
pub struct TypeDisplay<'a> {
    pub(crate) ty: &'a Type,
    pub(crate) style: TypeDisplayStyle,
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
pub(crate) fn fmt_type(
    ty: &Type,
    style: TypeDisplayStyle,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
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
