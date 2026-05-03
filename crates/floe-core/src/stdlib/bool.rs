use super::{StdlibFn, Type, fun, stdlib_fn, tv};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);

    fns.extend([
        stdlib_fn!("Bool", "guard", [Type::Bool, t.clone(), fun(vec![], t.clone())], t.clone(), "$0 ? ($2)() : $1"),
    ]);
}
