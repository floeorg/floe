use super::{StdlibFn, Type, array_of, option_of, record_of, stdlib_fn, tv};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);
    let u = tv(1);

    fns.extend([
        stdlib_fn!("Record", "get", [record_of(t.clone(), u.clone()), t.clone()], option_of(u.clone()), "$0[$1]"),
        stdlib_fn!("Record", "has", [record_of(t.clone(), u.clone()), t.clone()], Type::Bool, "($1 in $0)"),
        stdlib_fn!("Record", "keys", [record_of(t.clone(), u.clone())], array_of(t.clone()), "Object.keys($0)"),
        stdlib_fn!("Record", "values", [record_of(t.clone(), u.clone())], array_of(u.clone()), "Object.values($0)"),
        stdlib_fn!("Record", "entries", [record_of(t.clone(), u.clone())], array_of(Type::Tuple(vec![t.clone(), u.clone()])), "Object.entries($0)"),
        stdlib_fn!("Record", "size", [record_of(t.clone(), u.clone())], Type::Number, "Object.keys($0).length"),
        stdlib_fn!("Record", "isEmpty", [record_of(t.clone(), u.clone())], Type::Bool, "Object.keys($0).length === 0"),
    ]);
}
