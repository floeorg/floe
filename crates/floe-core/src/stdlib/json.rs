use super::{StdlibFn, Type, err_value, ok_value, result_of, stdlib_fn, tv};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);

    fns.extend([
        stdlib_fn!("JSON", "stringify", [t.clone()], Type::String, "JSON.stringify($0)"),
        stdlib_fn!(
            "JSON", "parse",
            [Type::String],
            result_of(t.clone(), Type::Named("ParseError".to_string())),
            concat!(
                "(() => { try { return ", ok_value!("JSON.parse($0)"), "; ",
                "} catch (e) { return ", err_value!("{ message: String(e) }"), "; ",
                "} })()"
            )
        ),
    ]);
}
