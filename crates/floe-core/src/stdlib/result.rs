use super::{StdlibFn, Type, array_of, fun, option_of, result_of, stdlib_fn, tv};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);
    let u = tv(1);

    fns.extend([
        stdlib_fn!("Result", "map", [result_of(t.clone(), u.clone()), fun(vec![t.clone()], tv(2))], result_of(tv(2), u.clone()), "$0.ok ? { ok: true as const, value: ($1)($0.value) } : $0"),
        stdlib_fn!("Result", "mapErr", [result_of(t.clone(), u.clone()), fun(vec![u.clone()], tv(2))], result_of(t.clone(), tv(2)), "$0.ok ? $0 : { ok: false as const, error: ($1)($0.error) }"),
        stdlib_fn!("Result", "flatMap", [result_of(t.clone(), u.clone()), fun(vec![t.clone()], result_of(tv(2), u.clone()))], result_of(tv(2), u.clone()), "$0.ok ? ($1)($0.value) : $0"),
        stdlib_fn!("Result", "unwrapOr", [result_of(t.clone(), u.clone()), t.clone()], t.clone(), "$0.ok ? $0.value : $1"),
        stdlib_fn!("Result", "isOk", [result_of(t.clone(), u.clone())], Type::Bool, "$0.ok"),
        stdlib_fn!("Result", "isErr", [result_of(t.clone(), u.clone())], Type::Bool, "!$0.ok"),
        stdlib_fn!("Result", "toOption", [result_of(t.clone(), u.clone())], option_of(t.clone()), "$0.ok ? $0.value : undefined"),
        stdlib_fn!("Result", "filter", [result_of(t.clone(), u.clone()), fun(vec![t.clone()], Type::Bool), u.clone()], result_of(t.clone(), u.clone()), "$0.ok && ($1)($0.value) ? $0 : $0.ok ? { ok: false as const, error: $2 } : $0"),
        stdlib_fn!("Result", "or", [result_of(t.clone(), u.clone()), result_of(t.clone(), u.clone())], result_of(t.clone(), u.clone()), "$0.ok ? $0 : $1"),
        stdlib_fn!("Result", "mapOr", [result_of(t.clone(), u.clone()), tv(2), fun(vec![t.clone()], tv(2))], tv(2), "$0.ok ? ($2)($0.value) : $1"),
        stdlib_fn!("Result", "flatten", [result_of(result_of(t.clone(), u.clone()), u.clone())], result_of(t.clone(), u.clone()), "$0.ok ? $0.value : $0"),
        stdlib_fn!("Result", "zip", [result_of(t.clone(), u.clone()), result_of(tv(2), u.clone())], result_of(Type::Tuple(vec![t.clone(), tv(2)]), u.clone()), "$0.ok && $1.ok ? { ok: true as const, value: [$0.value, $1.value] as const } : !$0.ok ? $0 : $1"),
        stdlib_fn!("Result", "inspect", [result_of(t.clone(), u.clone()), fun(vec![t.clone()], Type::Unit)], result_of(t.clone(), u.clone()), "(() => { const _v = $0; if (_v.ok) ($1)(_v.value); return _v; })()"),
        stdlib_fn!("Result", "inspectErr", [result_of(t.clone(), u.clone()), fun(vec![u.clone()], Type::Unit)], result_of(t.clone(), u.clone()), "(() => { const _v = $0; if (!_v.ok) ($1)(_v.error); return _v; })()"),
        stdlib_fn!("Result", "all", [array_of(result_of(t.clone(), u.clone()))], result_of(array_of(t.clone()), u.clone()), "(() => { const _a = $0; const _r = []; for (const _v of _a) { if (!_v.ok) return _v; _r.push(_v.value); } return { ok: true as const, value: _r }; })()"),
        stdlib_fn!("Result", "any", [array_of(result_of(t.clone(), u.clone()))], result_of(t.clone(), array_of(u.clone())), "(() => { const _a = $0; const _e = []; for (const _v of _a) { if (_v.ok) return _v; _e.push(_v.error); } return { ok: false as const, error: _e }; })()"),
        stdlib_fn!("Result", "values", [array_of(result_of(t.clone(), u.clone()))], array_of(t.clone()), "(() => { const _a = $0; const _r = []; for (const _v of _a) { if (_v.ok) _r.push(_v.value); } return _r; })()"),
        stdlib_fn!("Result", "partition", [array_of(result_of(t.clone(), u.clone()))], Type::Tuple(vec![array_of(t.clone()), array_of(u.clone())]), "(() => { const _a = $0; const _ok = []; const _err = []; for (const _v of _a) { if (_v.ok) _ok.push(_v.value); else _err.push(_v.error); } return [_ok, _err] as const; })()"),
        stdlib_fn!("Result", "guard", [result_of(t.clone(), u.clone()), fun(vec![u.clone()], tv(2)), fun(vec![t.clone()], tv(2))], tv(2), "$0.ok ? ($2)($0.value) : ($1)($0.error)"),
        stdlib_fn!("Result", "orElse", [result_of(t.clone(), u.clone()), fun(vec![u.clone()], result_of(t.clone(), tv(2)))], result_of(t.clone(), tv(2)), "$0.ok ? $0 : ($1)($0.error)"),
    ]);
}
