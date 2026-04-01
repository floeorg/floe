use super::*;

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let t = tv(0);
    let u = tv(1);

    fns.extend([
        stdlib_fn!("Array", "sort", [array_of(t.clone())], array_of(t.clone()), "[...$0].sort((a, b) => a - b)"),
        stdlib_fn!("Array", "sortBy", [array_of(t.clone()), fun(vec![t.clone()], Type::Number)], array_of(t.clone()), "[...$0].sort((a, b) => ($1)(a) - ($1)(b))"),
        stdlib_fn!("Array", "map", [array_of(t.clone()), fun(vec![t.clone()], u.clone())], array_of(u.clone()), "$0.map($1)"),
        stdlib_fn!("Array", "filter", [array_of(t.clone()), fun(vec![t.clone()], Type::Bool)], array_of(t.clone()), "$0.filter($1)"),
        stdlib_fn!("Array", "find", [array_of(t.clone()), fun(vec![t.clone()], Type::Bool)], option_of(t.clone()), "$0.find($1)"),
        stdlib_fn!("Array", "findIndex", [array_of(t.clone()), fun(vec![t.clone()], Type::Bool)], option_of(Type::Number), "(() => { const _i = $0.findIndex($1); return _i === -1 ? undefined : _i; })()"),
        stdlib_fn!("Array", "flatMap", [array_of(t.clone()), fun(vec![t.clone()], array_of(u.clone()))], array_of(u.clone()), "$0.flatMap($1)"),
        stdlib_fn!("Array", "at", [array_of(t.clone()), Type::Number], option_of(t.clone()), "$0[$1]"),
        stdlib_fn!("Array", "contains", [array_of(t.clone()), t.clone()], Type::Bool, "$0.some((_item) => __floeEq(_item, $1))"),
        stdlib_fn!("Array", "head", [array_of(t.clone())], option_of(t.clone()), "$0[0]"),
        stdlib_fn!("Array", "last", [array_of(t.clone())], option_of(t.clone()), "$0[$0.length - 1]"),
        stdlib_fn!("Array", "take", [array_of(t.clone()), Type::Number], array_of(t.clone()), "$0.slice(0, $1)"),
        stdlib_fn!("Array", "drop", [array_of(t.clone()), Type::Number], array_of(t.clone()), "$0.slice($1)"),
        stdlib_fn!("Array", "reverse", [array_of(t.clone())], array_of(t.clone()), "[...$0].reverse()"),
        stdlib_fn!("Array", "reduce", [array_of(t.clone()), fun(vec![u.clone(), t.clone()], u.clone()), u.clone()], u.clone(), "$0.reduce($1, $2)"),
        stdlib_fn!("Array", "length", [array_of(t.clone())], Type::Number, "$0.length"),
        stdlib_fn!("Array", "concat", [array_of(t.clone()), array_of(t.clone())], array_of(t.clone()), "[...$0, ...$1]"),
        stdlib_fn!("Array", "append", [array_of(t.clone()), t.clone()], array_of(t.clone()), "[...$0, $1]"),
        stdlib_fn!("Array", "prepend", [array_of(t.clone()), t.clone()], array_of(t.clone()), "[$1, ...$0]"),
        stdlib_fn!("Array", "any", [array_of(t.clone()), fun(vec![t.clone()], Type::Bool)], Type::Bool, "$0.some($1)"),
        stdlib_fn!("Array", "all", [array_of(t.clone()), fun(vec![t.clone()], Type::Bool)], Type::Bool, "$0.every($1)"),
        stdlib_fn!("Array", "sum", [array_of(Type::Number)], Type::Number, "$0.reduce((a, b) => a + b, 0)"),
        stdlib_fn!("Array", "join", [array_of(Type::String), Type::String], Type::String, "$0.join($1)"),
        stdlib_fn!("Array", "isEmpty", [array_of(t.clone())], Type::Bool, "$0.length === 0"),
        stdlib_fn!("Array", "chunk", [array_of(t.clone()), Type::Number], array_of(array_of(t.clone())), "(() => { const _a = $0; const _n = $1; const _r = []; for (let _i = 0; _i < _a.length; _i += _n) _r.push(_a.slice(_i, _i + _n)); return _r; })()"),
        stdlib_fn!("Array", "unique", [array_of(t.clone())], array_of(t.clone()), "[...new Set($0)]"),
        stdlib_fn!("Array", "groupBy", [array_of(t.clone()), fun(vec![t.clone()], Type::String)], record_of(Type::String, array_of(t.clone())), "Object.groupBy($0, $1)"),
        stdlib_fn!("Array", "zip", [array_of(t.clone()), array_of(u.clone())], array_of(Type::Tuple(vec![t.clone(), u.clone()])), "$0.map((_v, _i) => [_v, $1[_i]] as const)"),
        stdlib_fn!("Array", "from", [t.clone(), fun(vec![t.clone(), Type::Number], u.clone())], array_of(u.clone()), "Array.from($0, $1)"),
        stdlib_fn!("Array", "mapResult", [array_of(t.clone()), fun(vec![t.clone()], result_of(u.clone(), tv(2)))], result_of(array_of(u.clone()), tv(2)), "(() => { const _a = $0; const _f = $1; const _r = []; for (const _v of _a) { const _res = _f(_v); if (!_res.ok) return _res; _r.push(_res.value); } return { ok: true as const, value: _r }; })()"),
        stdlib_fn!("Array", "filterMap", [array_of(t.clone()), fun(vec![t.clone()], option_of(u.clone()))], array_of(u.clone()), "(() => { const _a = $0; const _f = $1; const _r = []; for (const _v of _a) { const _m = _f(_v); if (_m !== undefined) _r.push(_m); } return _r; })()"),
        stdlib_fn!("Array", "partition", [array_of(t.clone()), fun(vec![t.clone()], Type::Bool)], Type::Tuple(vec![array_of(t.clone()), array_of(t.clone())]), "(() => { const _a = $0; const _f = $1; const _t = []; const _u = []; for (const _v of _a) { (_f(_v) ? _t : _u).push(_v); } return [_t, _u] as const; })()"),
        stdlib_fn!("Array", "flatten", [array_of(array_of(t.clone()))], array_of(t.clone()), "$0.flat()"),
        stdlib_fn!("Array", "findLast", [array_of(t.clone()), fun(vec![t.clone()], Type::Bool)], option_of(t.clone()), "$0.findLast($1)"),
        stdlib_fn!("Array", "takeWhile", [array_of(t.clone()), fun(vec![t.clone()], Type::Bool)], array_of(t.clone()), "(() => { const _a = $0; const _f = $1; const _r = []; for (const _v of _a) { if (!_f(_v)) break; _r.push(_v); } return _r; })()"),
        stdlib_fn!("Array", "dropWhile", [array_of(t.clone()), fun(vec![t.clone()], Type::Bool)], array_of(t.clone()), "(() => { const _a = $0; const _f = $1; let _i = 0; while (_i < _a.length && _f(_a[_i])) _i++; return _a.slice(_i); })()"),
        stdlib_fn!("Array", "intersperse", [array_of(t.clone()), t.clone()], array_of(t.clone()), "(() => { const _a = $0; const _s = $1; const _r = []; for (let _i = 0; _i < _a.length; _i++) { if (_i > 0) _r.push(_s); _r.push(_a[_i]); } return _r; })()"),
    ]);
}
