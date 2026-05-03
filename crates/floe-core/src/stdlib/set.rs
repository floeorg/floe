use super::{StdlibFn, Type, array_of, set_of, stdlib_fn, tv};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);

    fns.extend([
        stdlib_fn!("Set", "empty", [], set_of(t.clone()), "new Set()"),
        stdlib_fn!("Set", "fromArray", [array_of(t.clone())], set_of(t.clone()), "new Set($0)"),
        stdlib_fn!("Set", "toArray", [set_of(t.clone())], array_of(t.clone()), "[...$0]"),
        stdlib_fn!("Set", "add", [set_of(t.clone()), t.clone()], set_of(t.clone()), "new Set([...$0, $1])"),
        stdlib_fn!("Set", "remove", [set_of(t.clone()), t.clone()], set_of(t.clone()), "new Set([...$0].filter(x => x !== $1))"),
        stdlib_fn!("Set", "has", [set_of(t.clone()), t.clone()], Type::Bool, "$0.has($1)"),
        stdlib_fn!("Set", "size", [set_of(t.clone())], Type::Number, "$0.size"),
        stdlib_fn!("Set", "isEmpty", [set_of(t.clone())], Type::Bool, "$0.size === 0"),
        stdlib_fn!("Set", "union", [set_of(t.clone()), set_of(t.clone())], set_of(t.clone()), "new Set([...$0, ...$1])"),
        stdlib_fn!("Set", "intersect", [set_of(t.clone()), set_of(t.clone())], set_of(t.clone()), "new Set([...$0].filter(x => $1.has(x)))"),
        stdlib_fn!("Set", "diff", [set_of(t.clone()), set_of(t.clone())], set_of(t.clone()), "new Set([...$0].filter(x => !$1.has(x)))"),
    ]);
}
