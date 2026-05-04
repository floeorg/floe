use super::{StdlibFn, Type, array_of, err_value, ok_value, promise_of, result_of, stdlib_fn, tv};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);
    let u = tv(1);

    fns.extend([
        stdlib_fn!("Promise", "await", [promise_of(t.clone())], t.clone(), "(await $0)"),
        stdlib_fn!("Promise", "all", [array_of(promise_of(t.clone()))], promise_of(array_of(t.clone())), "Promise.all($0)"),
        stdlib_fn!("Promise", "race", [array_of(promise_of(t.clone()))], promise_of(t.clone()), "Promise.race($0)"),
        stdlib_fn!("Promise", "any", [array_of(promise_of(t.clone()))], promise_of(t.clone()), "Promise.any($0)"),
        stdlib_fn!(
            "Promise", "allSettled",
            [array_of(promise_of(t.clone()))],
            promise_of(array_of(result_of(t.clone(), Type::Named("Error".to_string())))),
            concat!(
                "Promise.allSettled($0).then(_a => _a.map(_v => _v.status === \"fulfilled\" ? ",
                ok_value!("_v.value"),
                " : ",
                err_value!("_v.reason instanceof Error ? _v.reason : new Error(String(_v.reason))"),
                "))"
            )
        ),
        stdlib_fn!("Promise", "resolve", [t.clone()], promise_of(t.clone()), "Promise.resolve($0)"),
        stdlib_fn!("Promise", "reject", [u.clone()], promise_of(t.clone()), "Promise.reject($0)"),
        stdlib_fn!("Promise", "delay", [Type::Number], promise_of(Type::Unit), "new Promise(_r => setTimeout(_r, $0))"),
    ]);
}
