use super::*;

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);
    let u = tv(1);

    fns.extend([
        stdlib_fn!("Map", "empty", [], map_of(t.clone(), u.clone()), "new Map()"),
        stdlib_fn!("Map", "fromArray", [array_of(Type::Tuple(vec![t.clone(), u.clone()]))], map_of(t.clone(), u.clone()), "new Map($0)"),
        stdlib_fn!("Map", "get", [map_of(t.clone(), u.clone()), t.clone()], option_of(u.clone()), "$0.has($1) ? $0.get($1) : undefined"),
        stdlib_fn!("Map", "set", [map_of(t.clone(), u.clone()), t.clone(), u.clone()], map_of(t.clone(), u.clone()), "new Map([...$0, [$1, $2]])"),
        stdlib_fn!("Map", "remove", [map_of(t.clone(), u.clone()), t.clone()], map_of(t.clone(), u.clone()), "new Map([...$0].filter(([k]) -> k !== $1))"),
        stdlib_fn!("Map", "has", [map_of(t.clone(), u.clone()), t.clone()], Type::Bool, "$0.has($1)"),
        stdlib_fn!("Map", "keys", [map_of(t.clone(), u.clone())], array_of(t.clone()), "[...$0.keys()]"),
        stdlib_fn!("Map", "values", [map_of(t.clone(), u.clone())], array_of(u.clone()), "[...$0.values()]"),
        stdlib_fn!("Map", "entries", [map_of(t.clone(), u.clone())], array_of(Type::Tuple(vec![t.clone(), u.clone()])), "[...$0.entries()]"),
        stdlib_fn!("Map", "size", [map_of(t.clone(), u.clone())], Type::Number, "$0.size"),
        stdlib_fn!("Map", "isEmpty", [map_of(t.clone(), u.clone())], Type::Bool, "$0.size === 0"),
        stdlib_fn!("Map", "merge", [map_of(t.clone(), u.clone()), map_of(t.clone(), u.clone())], map_of(t.clone(), u.clone()), "new Map([...$0, ...$1])"),
    ]);
}
