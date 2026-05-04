use super::{StdlibFn, Type, result_of, stdlib_fn, try_catch_result, tv};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);

    fns.extend([
        stdlib_fn!("JSON", "stringify", [t.clone()], Type::String, "JSON.stringify($0)"),
        stdlib_fn!(
            "JSON", "parse",
            [Type::String],
            result_of(t.clone(), Type::Named("ParseError".to_string())),
            try_catch_result!("JSON.parse($0)")
        ),
    ]);
}
