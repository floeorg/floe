use super::*;

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    fns.extend([
        stdlib_fn!("String", "trim", [Type::String], Type::String, "$0.trim()"),
        stdlib_fn!("String", "trimStart", [Type::String], Type::String, "$0.trimStart()"),
        stdlib_fn!("String", "trimEnd", [Type::String], Type::String, "$0.trimEnd()"),
        stdlib_fn!("String", "split", [Type::String, Type::String], array_of(Type::String), "$0.split($1)"),
        stdlib_fn!("String", "replace", [Type::String, Type::String, Type::String], Type::String, "$0.replace($1, $2)"),
        stdlib_fn!("String", "startsWith", [Type::String, Type::String], Type::Bool, "$0.startsWith($1)"),
        stdlib_fn!("String", "endsWith", [Type::String, Type::String], Type::Bool, "$0.endsWith($1)"),
        stdlib_fn!("String", "contains", [Type::String, Type::String], Type::Bool, "$0.includes($1)"),
        stdlib_fn!("String", "toUpperCase", [Type::String], Type::String, "$0.toUpperCase()"),
        stdlib_fn!("String", "toLowerCase", [Type::String], Type::String, "$0.toLowerCase()"),
        stdlib_fn!("String", "length", [Type::String], Type::Number, "$0.length"),
        stdlib_fn!("String", "slice", [Type::String, Type::Number, Type::Number], Type::String, "$0.slice($1, $2)"),
        stdlib_fn!("String", "padStart", [Type::String, Type::Number, Type::String], Type::String, "$0.padStart($1, $2)"),
        stdlib_fn!("String", "padEnd", [Type::String, Type::Number, Type::String], Type::String, "$0.padEnd($1, $2)"),
        stdlib_fn!("String", "repeat", [Type::String, Type::Number], Type::String, "$0.repeat($1)"),
        stdlib_fn!("String", "localeCompare", [Type::String, Type::String], Type::Number, "$0.localeCompare($1)"),
    ]);
}
