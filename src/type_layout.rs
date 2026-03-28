//! Centralizes how Floe types map to TypeScript runtime representations.
//!
//! Every question about runtime shape — discriminant fields, field accessors,
//! constructor layouts — should be answered by this module instead of being
//! hardcoded across checker and codegen.

// ── Runtime field names ──────────────────────────────────────────

/// Discriminant field for tagged union variants: `{ tag: "VariantName", ... }`
pub const TAG_FIELD: &str = "tag";
/// Discriminant field for Result/Option types: `{ ok: true/false, ... }`
pub const OK_FIELD: &str = "ok";
/// Value field for Ok/Some: `{ ok: true, value: ... }`
pub const VALUE_FIELD: &str = "value";
/// Error field for Err: `{ ok: false, error: ... }`
pub const ERROR_FIELD: &str = "error";

// ── Built-in type name constants ─────────────────────────────────

pub const TYPE_NUMBER: &str = "number";
pub const TYPE_STRING: &str = "string";
pub const TYPE_BOOLEAN: &str = "boolean";
pub const TYPE_UNIT: &str = "()";
pub const TYPE_UNDEFINED: &str = "undefined";
pub const TYPE_UNKNOWN: &str = "unknown";
pub const TYPE_OPTION: &str = "Option";
pub const TYPE_SETTABLE: &str = "Settable";
pub const TYPE_RESULT: &str = "Result";
pub const TYPE_ARRAY: &str = "Array";
pub const TYPE_ERROR: &str = "Error";
pub const TYPE_RESPONSE: &str = "Response";

// ── Stdlib module name constants ─────────────────────────────────

pub const MOD_ARRAY: &str = "Array";
pub const MOD_STRING: &str = "String";
pub const MOD_NUMBER: &str = "Number";
pub const MOD_OPTION: &str = "Option";
pub const MOD_RESULT: &str = "Result";
pub const MOD_MAP: &str = "Map";
pub const MOD_SET: &str = "Set";
pub const MOD_DATE: &str = "Date";

// ── Variant classification ───────────────────────────────────────

/// How a variant is discriminated and its fields accessed at runtime.
pub enum VariantLayout {
    /// Result Ok: discriminated by `.ok === true`, value in `.value`
    Ok,
    /// Result Err: discriminated by `.ok === false`, error in `.error`
    Err,
    /// Option Some: discriminated by `!= null`, value IS the subject itself
    OptionSome,
    /// Option None: discriminated by `== null`
    OptionNone,
    /// Regular tagged union variant: discriminated by `.tag === "Name"`
    Tagged,
}

/// Classify a variant name into its runtime layout.
///
/// `Ok`/`Err` use the `{ ok: true/false }` discriminant.
/// `Some`/`None` use the `T | null | undefined` representation:
/// `Some(x)` is just `x`, `None` is `undefined`.
pub fn variant_layout(name: &str) -> VariantLayout {
    match name {
        "Ok" => VariantLayout::Ok,
        "Err" => VariantLayout::Err,
        "Some" => VariantLayout::OptionSome,
        "None" => VariantLayout::OptionNone,
        _ => VariantLayout::Tagged,
    }
}

/// Returns the JS condition string for testing a variant at runtime.
/// `subject` is the expression string being matched against.
pub fn variant_discriminant(name: &str, subject: &str) -> String {
    match variant_layout(name) {
        VariantLayout::Ok => format!("{subject}.{OK_FIELD} === true"),
        VariantLayout::Err => format!("{subject}.{OK_FIELD} === false"),
        VariantLayout::OptionSome => format!("{subject} != null"),
        VariantLayout::OptionNone => format!("{subject} == null"),
        VariantLayout::Tagged => format!("{subject}.{TAG_FIELD} === \"{name}\""),
    }
}

/// Returns the field accessor for a variant's field at the given index.
///
/// - Ok/Some with 1 field: `.value`
/// - Err with 1 field: `.error`
/// - Named field: `.fieldName`
/// - Single unnamed field: `.value`
/// - Multiple unnamed fields: `._0`, `._1`, etc.
pub fn variant_field_accessor(
    name: &str,
    field_index: usize,
    total_fields: usize,
    field_names: Option<&[String]>,
    subject: &str,
) -> String {
    match variant_layout(name) {
        VariantLayout::Ok if total_fields == 1 => {
            format!("{subject}.{VALUE_FIELD}")
        }
        VariantLayout::Err if total_fields == 1 => {
            format!("{subject}.{ERROR_FIELD}")
        }
        // Ok/Err always have exactly 1 field in Floe
        VariantLayout::Ok | VariantLayout::Err => {
            debug_assert!(false, "Ok/Err variants should have exactly 1 field");
            format!("{subject}.{VALUE_FIELD}")
        }
        // Some(x): the value IS the subject (Option is T | undefined)
        VariantLayout::OptionSome if total_fields == 1 => subject.to_string(),
        VariantLayout::OptionSome | VariantLayout::OptionNone => subject.to_string(),
        VariantLayout::Tagged => {
            if let Some(names) = field_names
                && let Some(fname) = names.get(field_index)
            {
                format!("{subject}.{fname}")
            } else if total_fields == 1 {
                format!("{subject}.{VALUE_FIELD}")
            } else {
                format!("{subject}._{field_index}")
            }
        }
    }
}

// ── Type → stdlib module mapping ─────────────────────────────

/// Map a checker `Type` to the corresponding stdlib module name.
/// Used by both checker (for type-directed pipe validation) and
/// codegen (for type-directed pipe emission).
pub fn type_to_stdlib_module(ty: &crate::checker::Type) -> Option<&'static str> {
    use crate::checker::Type;
    match ty {
        Type::Array(_) => Some(MOD_ARRAY),
        Type::Map { .. } => Some(MOD_MAP),
        Type::Set { .. } => Some(MOD_SET),
        Type::String => Some(MOD_STRING),
        Type::Number => Some(MOD_NUMBER),
        Type::Option(_) => Some(MOD_OPTION),
        Type::Result { .. } => Some(MOD_RESULT),
        Type::Named(name) if name == "Date" => Some(MOD_DATE),
        _ => None,
    }
}
