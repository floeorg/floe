use super::*;

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);

    fns.extend([
        stdlib_fn!("JSON", "stringify", [t.clone()], Type::String, "JSON.stringify($0)"),
        stdlib_fn!("JSON", "parse", [Type::String], result_of(t.clone(), Type::Named("ParseError".to_string())), "(() => { try { return { ok: true as const, value: JSON.parse($0) }; } catch (e) { return { ok: false as const, error: { message: String(e) } }; } })()"),
    ]);
}
