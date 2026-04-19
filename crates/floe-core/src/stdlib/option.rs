use super::*;

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);
    let u = tv(1);

    fns.extend([
        stdlib_fn!("Option", "map", [option_of(t.clone()), fun(vec![t.clone()], u.clone())], option_of(u.clone()), "$0 != null ? ($1)($0) : undefined"),
        stdlib_fn!("Option", "flatMap", [option_of(t.clone()), fun(vec![t.clone()], option_of(u.clone()))], option_of(u.clone()), "$0 != null ? ($1)($0) : undefined"),
        stdlib_fn!("Option", "unwrapOr", [option_of(t.clone()), t.clone()], t.clone(), "$0 != null ? $0 : $1"),
        stdlib_fn!("Option", "isSome", [option_of(t.clone())], Type::Bool, "$0 != null"),
        stdlib_fn!("Option", "isNone", [option_of(t.clone())], Type::Bool, "$0 == null"),
        stdlib_fn!("Option", "toResult", [option_of(t.clone()), u.clone()], result_of(t.clone(), u.clone()), "$0 != null ? { ok: true as const, value: $0 } : { ok: false as const, error: $1 }"),
        stdlib_fn!("Option", "filter", [option_of(t.clone()), fun(vec![t.clone()], Type::Bool)], option_of(t.clone()), "$0 != null && ($1)($0) ? $0 : undefined"),
        stdlib_fn!("Option", "or", [option_of(t.clone()), option_of(t.clone())], option_of(t.clone()), "$0 != null ? $0 : $1"),
        stdlib_fn!("Option", "mapOr", [option_of(t.clone()), u.clone(), fun(vec![t.clone()], u.clone())], u.clone(), "$0 != null ? ($2)($0) : $1"),
        stdlib_fn!("Option", "flatten", [option_of(option_of(t.clone()))], option_of(t.clone()), "$0"),
        stdlib_fn!("Option", "zip", [option_of(t.clone()), option_of(u.clone())], option_of(Type::Tuple(vec![t.clone(), u.clone()])), "$0 != null && $1 != null ? [$0, $1] as const : undefined"),
        stdlib_fn!("Option", "inspect", [option_of(t.clone()), fun(vec![t.clone()], Type::Unit)], option_of(t.clone()), "(() => { const _v = $0; if (_v != null) ($1)(_v); return _v; })()"),
        stdlib_fn!("Option", "toErr", [option_of(t.clone())], result_of(Type::Unit, t.clone()), "$0 != null ? { ok: false as const, error: $0 } : { ok: true as const, value: undefined }"),
        stdlib_fn!("Option", "all", [array_of(option_of(t.clone()))], option_of(array_of(t.clone())), "(() => { const _a = $0; const _r = []; for (const _v of _a) { if (_v == null) return undefined; _r.push(_v); } return _r; })()"),
        stdlib_fn!("Option", "any", [array_of(option_of(t.clone()))], option_of(t.clone()), "$0.find(_v -> _v != null)"),
        stdlib_fn!("Option", "values", [array_of(option_of(t.clone()))], array_of(t.clone()), "(() => { const _a = $0; const _r = []; for (const _v of _a) { if (_v != null) _r.push(_v); } return _r; })()"),
        stdlib_fn!("Option", "guard", [option_of(t.clone()), u.clone(), fun(vec![t.clone()], u.clone())], u.clone(), "$0 != null ? ($2)($0) : $1"),
        stdlib_fn!("Option", "orElse", [option_of(t.clone()), fun(vec![], option_of(t.clone()))], option_of(t.clone()), "$0 != null ? $0 : ($1)()"),
    ]);
}
