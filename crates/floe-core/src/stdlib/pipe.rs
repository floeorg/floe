use super::{StdlibFn, Type, fun, stdlib_fn, tv};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);

    fns.extend([
        stdlib_fn!("Pipe", "tap", [t.clone(), fun(vec![t.clone()], Type::Unit)], t.clone(), "(() => { const _v = $0; ($1)(_v); return _v; })()"),
    ]);
}
