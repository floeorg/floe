use super::*;

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    fns.extend([
        stdlib_fn!("Number", "parse", [Type::String], result_of(Type::Number, Type::Named("ParseError".to_string())), "(() => { const _n = Number($0); return Number.isNaN(_n) || $0.trim() === \"\" ? { ok: false as const, error: { message: `Failed to parse \"${$0}\" as number` } } : { ok: true as const, value: _n }; })()"),
        stdlib_fn!("Number", "clamp", [Type::Number, Type::Number, Type::Number], Type::Number, "Math.min(Math.max($0, $1), $2)"),
        stdlib_fn!("Number", "isFinite", [Type::Number], Type::Bool, "Number.isFinite($0)"),
        stdlib_fn!("Number", "isInteger", [Type::Number], Type::Bool, "Number.isInteger($0)"),
        stdlib_fn!("Number", "toFixed", [Type::Number, Type::Number], Type::String, "$0.toFixed($1)"),
        stdlib_fn!("Number", "toString", [Type::Number], Type::String, "String($0)"),
    ]);
}
