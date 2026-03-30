//! Floe standard library — built-in functions known to the compiler.
//!
//! These functions exist only at compile time. The checker uses them for type
//! checking, and the codegen inlines them as vanilla TypeScript. No runtime
//! dependency is emitted.

use crate::checker::Type;

/// A standard library function definition.
#[derive(Debug, Clone)]
pub struct StdlibFn {
    /// Module name: "Array", "Option", "Result", "String", "Number"
    pub module: &'static str,
    /// Function name: "sort", "map", "unwrapOr", etc.
    pub name: &'static str,
    /// Parameter types. The first param is the "receiver" for pipe ergonomics.
    /// Generic params use Type::Var(0), Type::Var(1), etc.
    pub params: Vec<Type>,
    /// Return type.
    pub return_type: Type,
    /// Codegen template. Placeholders: `$0` = first arg, `$1` = second arg, etc.
    /// `$..` = all args comma-separated (for variadic functions like `Console.log`).
    /// Example: `[...$0].sort((a, b) => a - b)` for Array.sort
    pub codegen: &'static str,
}

impl StdlibFn {
    /// Returns true if this function accepts any number of arguments.
    /// Inferred from the `$..` placeholder in the codegen template.
    pub fn is_variadic(&self) -> bool {
        self.codegen.contains("$..")
    }
}

/// Registry of all standard library functions.
#[derive(Default)]
pub struct StdlibRegistry {
    functions: Vec<StdlibFn>,
}

impl StdlibRegistry {
    pub fn new() -> Self {
        Self {
            functions: build_stdlib(),
        }
    }

    /// Look up a stdlib function by module and name.
    pub fn lookup(&self, module: &str, name: &str) -> Option<&StdlibFn> {
        self.functions
            .iter()
            .find(|f| f.module == module && f.name == name)
    }

    /// Get all functions in a module (for autocomplete).
    pub fn module_functions(&self, module: &str) -> Vec<&StdlibFn> {
        self.functions
            .iter()
            .filter(|f| f.module == module)
            .collect()
    }

    /// Look up a stdlib function by name alone (for type-directed pipe resolution).
    /// Returns all matches across modules.
    pub fn lookup_by_name(&self, name: &str) -> Vec<&StdlibFn> {
        self.functions.iter().filter(|f| f.name == name).collect()
    }

    /// Get all stdlib functions.
    pub fn all_functions(&self) -> &[StdlibFn] {
        &self.functions
    }

    /// Check if a name is a stdlib module.
    pub fn is_module(&self, name: &str) -> bool {
        self.functions.iter().any(|f| f.module == name)
    }
}

/// Type variable helpers for generic signatures.
fn tv(n: usize) -> Type {
    Type::Var(n)
}
fn array_of(t: Type) -> Type {
    Type::Array(Box::new(t))
}
fn option_of(t: Type) -> Type {
    Type::Option(Box::new(t))
}
fn result_of(ok: Type, err: Type) -> Type {
    Type::Result {
        ok: Box::new(ok),
        err: Box::new(err),
    }
}
fn map_of(k: Type, v: Type) -> Type {
    Type::Map {
        key: Box::new(k),
        value: Box::new(v),
    }
}
fn set_of(t: Type) -> Type {
    Type::Set {
        element: Box::new(t),
    }
}
fn promise_of(t: Type) -> Type {
    Type::Promise(Box::new(t))
}
fn fun(params: Vec<Type>, ret: Type) -> Type {
    Type::Function {
        params,
        return_type: Box::new(ret),
    }
}

macro_rules! stdlib_fn {
    // Variadic: no params list
    ($module:expr, $name:expr, $ret:expr, $codegen:expr) => {
        StdlibFn {
            module: $module,
            name: $name,
            params: vec![],
            return_type: $ret,
            codegen: $codegen,
        }
    };
    // Fixed arity: explicit params list
    ($module:expr, $name:expr, [$($param:expr),*], $ret:expr, $codegen:expr) => {
        StdlibFn {
            module: $module,
            name: $name,
            params: vec![$($param),*],
            return_type: $ret,
            codegen: $codegen,
        }
    };
}

/// Build the full stdlib registry.
#[rustfmt::skip]
fn build_stdlib() -> Vec<StdlibFn> {
    let t = tv(0); // T
    let u = tv(1); // U

    vec![
        // ── Array ───────────────────────────────────────────────
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
        stdlib_fn!("Array", "groupBy", [array_of(t.clone()), fun(vec![t.clone()], Type::String)], Type::Named("Record".to_string()), "Object.groupBy($0, $1)"),
        stdlib_fn!("Array", "zip", [array_of(t.clone()), array_of(u.clone())], array_of(Type::Tuple(vec![t.clone(), u.clone()])), "$0.map((_v, _i) => [_v, $1[_i]] as const)"),
        stdlib_fn!("Array", "from", [t.clone(), fun(vec![t.clone(), Type::Number], u.clone())], array_of(u.clone()), "Array.from($0, $1)"),
        stdlib_fn!("Array", "mapResult", [array_of(t.clone()), fun(vec![t.clone()], result_of(u.clone(), tv(2)))], result_of(array_of(u.clone()), tv(2)), "(() => { const _a = $0; const _f = $1; const _r = []; for (const _v of _a) { const _res = _f(_v); if (!_res.ok) return _res; _r.push(_res.value); } return { ok: true as const, value: _r }; })()"),
        // ── Option ──────────────────────────────────────────────
        stdlib_fn!("Option", "map", [option_of(t.clone()), fun(vec![t.clone()], u.clone())], option_of(u.clone()), "$0 !== undefined ? ($1)($0) : undefined"),
        stdlib_fn!("Option", "flatMap", [option_of(t.clone()), fun(vec![t.clone()], option_of(u.clone()))], option_of(u.clone()), "$0 !== undefined ? ($1)($0) : undefined"),
        stdlib_fn!("Option", "unwrapOr", [option_of(t.clone()), t.clone()], t.clone(), "$0 !== undefined ? $0 : $1"),
        stdlib_fn!("Option", "isSome", [option_of(t.clone())], Type::Bool, "$0 !== undefined"),
        stdlib_fn!("Option", "isNone", [option_of(t.clone())], Type::Bool, "$0 === undefined"),
        stdlib_fn!("Option", "toResult", [option_of(t.clone()), u.clone()], result_of(t.clone(), u.clone()), "$0 !== undefined ? { ok: true as const, value: $0 } : { ok: false as const, error: $1 }"),
        stdlib_fn!("Option", "filter", [option_of(t.clone()), fun(vec![t.clone()], Type::Bool)], option_of(t.clone()), "$0 !== undefined && ($1)($0) ? $0 : undefined"),
        stdlib_fn!("Option", "unwrap", [option_of(t.clone())], t.clone(), "(() => { if ($0 === undefined) throw new Error(\"called Option.unwrap on None\"); return $0; })()"),
        stdlib_fn!("Option", "mapOr", [option_of(t.clone()), u.clone(), fun(vec![t.clone()], u.clone())], u.clone(), "$0 !== undefined ? ($2)($0) : $1"),
        stdlib_fn!("Option", "flatten", [option_of(option_of(t.clone()))], option_of(t.clone()), "$0"),
        stdlib_fn!("Option", "zip", [option_of(t.clone()), option_of(u.clone())], option_of(Type::Tuple(vec![t.clone(), u.clone()])), "$0 !== undefined && $1 !== undefined ? [$0, $1] as const : undefined"),
        stdlib_fn!("Option", "inspect", [option_of(t.clone()), fun(vec![t.clone()], Type::Unit)], option_of(t.clone()), "(() => { const _v = $0; if (_v !== undefined) ($1)(_v); return _v; })()"),
        stdlib_fn!("Option", "toErr", [option_of(t.clone())], result_of(Type::Unit, t.clone()), "$0 !== undefined ? { ok: false as const, error: $0 } : { ok: true as const, value: undefined }"),
        stdlib_fn!("Option", "all", [array_of(option_of(t.clone()))], option_of(array_of(t.clone())), "(() => { const _a = $0; const _r = []; for (const _v of _a) { if (_v === undefined) return undefined; _r.push(_v); } return _r; })()"),
        stdlib_fn!("Option", "any", [array_of(option_of(t.clone()))], option_of(t.clone()), "$0.find(_v => _v !== undefined)"),
        // ── Result ──────────────────────────────────────────────
        stdlib_fn!("Result", "map", [result_of(t.clone(), u.clone()), fun(vec![t.clone()], tv(2))], result_of(tv(2), u.clone()), "$0.ok ? { ok: true as const, value: ($1)($0.value) } : $0"),
        stdlib_fn!("Result", "mapErr", [result_of(t.clone(), u.clone()), fun(vec![u.clone()], tv(2))], result_of(t.clone(), tv(2)), "$0.ok ? $0 : { ok: false as const, error: ($1)($0.error) }"),
        stdlib_fn!("Result", "flatMap", [result_of(t.clone(), u.clone()), fun(vec![t.clone()], result_of(tv(2), u.clone()))], result_of(tv(2), u.clone()), "$0.ok ? ($1)($0.value) : $0"),
        stdlib_fn!("Result", "unwrapOr", [result_of(t.clone(), u.clone()), t.clone()], t.clone(), "$0.ok ? $0.value : $1"),
        stdlib_fn!("Result", "isOk", [result_of(t.clone(), u.clone())], Type::Bool, "$0.ok"),
        stdlib_fn!("Result", "isErr", [result_of(t.clone(), u.clone())], Type::Bool, "!$0.ok"),
        stdlib_fn!("Result", "toOption", [result_of(t.clone(), u.clone())], option_of(t.clone()), "$0.ok ? $0.value : undefined"),
        stdlib_fn!("Result", "filter", [result_of(t.clone(), u.clone()), fun(vec![t.clone()], Type::Bool), u.clone()], result_of(t.clone(), u.clone()), "$0.ok && ($1)($0.value) ? $0 : $0.ok ? { ok: false as const, error: $2 } : $0"),
        stdlib_fn!("Result", "unwrap", [result_of(t.clone(), u.clone())], t.clone(), "(() => { if (!$0.ok) throw $0.error; return $0.value; })()"),
        stdlib_fn!("Result", "unwrapErr", [result_of(t.clone(), u.clone())], u.clone(), "(() => { if ($0.ok) throw new Error(\"called Result.unwrapErr on Ok\"); return $0.error; })()"),
        stdlib_fn!("Result", "mapOr", [result_of(t.clone(), u.clone()), tv(2), fun(vec![t.clone()], tv(2))], tv(2), "$0.ok ? ($2)($0.value) : $1"),
        stdlib_fn!("Result", "flatten", [result_of(result_of(t.clone(), u.clone()), u.clone())], result_of(t.clone(), u.clone()), "$0.ok ? $0.value : $0"),
        stdlib_fn!("Result", "zip", [result_of(t.clone(), u.clone()), result_of(tv(2), u.clone())], result_of(Type::Tuple(vec![t.clone(), tv(2)]), u.clone()), "$0.ok && $1.ok ? { ok: true as const, value: [$0.value, $1.value] as const } : !$0.ok ? $0 : $1"),
        stdlib_fn!("Result", "inspect", [result_of(t.clone(), u.clone()), fun(vec![t.clone()], Type::Unit)], result_of(t.clone(), u.clone()), "(() => { const _v = $0; if (_v.ok) ($1)(_v.value); return _v; })()"),
        stdlib_fn!("Result", "inspectErr", [result_of(t.clone(), u.clone()), fun(vec![u.clone()], Type::Unit)], result_of(t.clone(), u.clone()), "(() => { const _v = $0; if (!_v.ok) ($1)(_v.error); return _v; })()"),
        stdlib_fn!("Result", "all", [array_of(result_of(t.clone(), u.clone()))], result_of(array_of(t.clone()), u.clone()), "(() => { const _a = $0; const _r = []; for (const _v of _a) { if (!_v.ok) return _v; _r.push(_v.value); } return { ok: true as const, value: _r }; })()"),
        stdlib_fn!("Result", "any", [array_of(result_of(t.clone(), u.clone()))], result_of(t.clone(), array_of(u.clone())), "(() => { const _a = $0; const _e = []; for (const _v of _a) { if (_v.ok) return _v; _e.push(_v.error); } return { ok: false as const, error: _e }; })()"),
        // ── String ──────────────────────────────────────────────
        stdlib_fn!("String", "trim", [Type::String], Type::String, "$0.trim()"),
        stdlib_fn!("String", "trimStart", [Type::String], Type::String, "$0.trimStart()"),
        stdlib_fn!("String", "trimEnd", [Type::String], Type::String, "$0.trimEnd()"),
        stdlib_fn!("String", "split", [Type::String, Type::String], array_of(Type::String), "$0.split($1)"),
        stdlib_fn!("String", "replace", [Type::String, Type::String, Type::String], Type::String, "$0.replace($1, $2)"),
        stdlib_fn!("String", "startsWith", [Type::String, Type::String], Type::Bool, "$0.startsWith($1)"),
        stdlib_fn!("String", "endsWith", [Type::String, Type::String], Type::Bool, "$0.endsWith($1)"),
        stdlib_fn!("String", "contains", [Type::String, Type::String], Type::Bool, "$0.includes($1)"),
        stdlib_fn!("String", "toUpperCase", [Type::String], Type::String, "$0.toUpperCase()"),
        stdlib_fn!("String", "toLowerCase", [Type::String], Type::String, "$0.toLowerCase()"),
        stdlib_fn!("String", "length", [Type::String], Type::Number, "$0.length"),
        stdlib_fn!("String", "slice", [Type::String, Type::Number, Type::Number], Type::String, "$0.slice($1, $2)"),
        stdlib_fn!("String", "padStart", [Type::String, Type::Number, Type::String], Type::String, "$0.padStart($1, $2)"),
        stdlib_fn!("String", "padEnd", [Type::String, Type::Number, Type::String], Type::String, "$0.padEnd($1, $2)"),
        stdlib_fn!("String", "repeat", [Type::String, Type::Number], Type::String, "$0.repeat($1)"),
        stdlib_fn!("String", "localeCompare", [Type::String, Type::String], Type::Number, "$0.localeCompare($1)"),
        // ── Number ──────────────────────────────────────────────
        stdlib_fn!("Number", "parse", [Type::String], result_of(Type::Number, Type::Named("ParseError".to_string())), "(() => { const _n = Number($0); return Number.isNaN(_n) || $0.trim() === \"\" ? { ok: false as const, error: { message: `Failed to parse \"${$0}\" as number` } } : { ok: true as const, value: _n }; })()"),
        stdlib_fn!("Number", "clamp", [Type::Number, Type::Number, Type::Number], Type::Number, "Math.min(Math.max($0, $1), $2)"),
        stdlib_fn!("Number", "isFinite", [Type::Number], Type::Bool, "Number.isFinite($0)"),
        stdlib_fn!("Number", "isInteger", [Type::Number], Type::Bool, "Number.isInteger($0)"),
        stdlib_fn!("Number", "toFixed", [Type::Number, Type::Number], Type::String, "$0.toFixed($1)"),
        stdlib_fn!("Number", "toString", [Type::Number], Type::String, "String($0)"),
        // ── Console ────────────────────────────────────────────
        stdlib_fn!("Console", "log", Type::Unit, "console.log($..)"),
        stdlib_fn!("Console", "warn", Type::Unit, "console.warn($..)"),
        stdlib_fn!("Console", "error", Type::Unit, "console.error($..)"),
        stdlib_fn!("Console", "info", Type::Unit, "console.info($..)"),
        stdlib_fn!("Console", "debug", Type::Unit, "console.debug($..)"),
        stdlib_fn!("Console", "time", [Type::String], Type::Unit, "console.time($0)"),
        stdlib_fn!("Console", "timeEnd", [Type::String], Type::Unit, "console.timeEnd($0)"),
        // ── Math ───────────────────────────────────────────────
        stdlib_fn!("Math", "floor", [Type::Number], Type::Number, "Math.floor($0)"),
        stdlib_fn!("Math", "ceil", [Type::Number], Type::Number, "Math.ceil($0)"),
        stdlib_fn!("Math", "round", [Type::Number], Type::Number, "Math.round($0)"),
        stdlib_fn!("Math", "abs", [Type::Number], Type::Number, "Math.abs($0)"),
        stdlib_fn!("Math", "min", [Type::Number, Type::Number], Type::Number, "Math.min($0, $1)"),
        stdlib_fn!("Math", "max", [Type::Number, Type::Number], Type::Number, "Math.max($0, $1)"),
        stdlib_fn!("Math", "pow", [Type::Number, Type::Number], Type::Number, "Math.pow($0, $1)"),
        stdlib_fn!("Math", "sqrt", [Type::Number], Type::Number, "Math.sqrt($0)"),
        stdlib_fn!("Math", "sign", [Type::Number], Type::Number, "Math.sign($0)"),
        stdlib_fn!("Math", "trunc", [Type::Number], Type::Number, "Math.trunc($0)"),
        stdlib_fn!("Math", "log", [Type::Number], Type::Number, "Math.log($0)"),
        stdlib_fn!("Math", "sin", [Type::Number], Type::Number, "Math.sin($0)"),
        stdlib_fn!("Math", "cos", [Type::Number], Type::Number, "Math.cos($0)"),
        stdlib_fn!("Math", "tan", [Type::Number], Type::Number, "Math.tan($0)"),
        stdlib_fn!("Math", "random", [], Type::Number, "Math.random()"),
        // ── Map ────────────────────────────────────────────────────
        stdlib_fn!("Map", "empty", [], map_of(t.clone(), u.clone()), "new Map()"),
        stdlib_fn!("Map", "fromArray", [array_of(Type::Tuple(vec![t.clone(), u.clone()]))], map_of(t.clone(), u.clone()), "new Map($0)"),
        stdlib_fn!("Map", "get", [map_of(t.clone(), u.clone()), t.clone()], option_of(u.clone()), "$0.has($1) ? $0.get($1) : undefined"),
        stdlib_fn!("Map", "set", [map_of(t.clone(), u.clone()), t.clone(), u.clone()], map_of(t.clone(), u.clone()), "new Map([...$0, [$1, $2]])"),
        stdlib_fn!("Map", "remove", [map_of(t.clone(), u.clone()), t.clone()], map_of(t.clone(), u.clone()), "new Map([...$0].filter(([k]) => k !== $1))"),
        stdlib_fn!("Map", "has", [map_of(t.clone(), u.clone()), t.clone()], Type::Bool, "$0.has($1)"),
        stdlib_fn!("Map", "keys", [map_of(t.clone(), u.clone())], array_of(t.clone()), "[...$0.keys()]"),
        stdlib_fn!("Map", "values", [map_of(t.clone(), u.clone())], array_of(u.clone()), "[...$0.values()]"),
        stdlib_fn!("Map", "entries", [map_of(t.clone(), u.clone())], array_of(Type::Tuple(vec![t.clone(), u.clone()])), "[...$0.entries()]"),
        stdlib_fn!("Map", "size", [map_of(t.clone(), u.clone())], Type::Number, "$0.size"),
        stdlib_fn!("Map", "isEmpty", [map_of(t.clone(), u.clone())], Type::Bool, "$0.size === 0"),
        stdlib_fn!("Map", "merge", [map_of(t.clone(), u.clone()), map_of(t.clone(), u.clone())], map_of(t.clone(), u.clone()), "new Map([...$0, ...$1])"),
        // ── Set ────────────────────────────────────────────────────
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
        // ── Date ───────────────────────────────────────────────────
        stdlib_fn!("Date", "now", [], Type::Named("Date".to_string()), "new Date()"),
        stdlib_fn!("Date", "from", [Type::String], Type::Named("Date".to_string()), "new Date($0)"),
        stdlib_fn!("Date", "fromMillis", [Type::Number], Type::Named("Date".to_string()), "new Date($0)"),
        stdlib_fn!("Date", "year", [Type::Named("Date".to_string())], Type::Number, "$0.getFullYear()"),
        stdlib_fn!("Date", "month", [Type::Named("Date".to_string())], Type::Number, "($0.getMonth() + 1)"),
        stdlib_fn!("Date", "day", [Type::Named("Date".to_string())], Type::Number, "$0.getDate()"),
        stdlib_fn!("Date", "hour", [Type::Named("Date".to_string())], Type::Number, "$0.getHours()"),
        stdlib_fn!("Date", "minute", [Type::Named("Date".to_string())], Type::Number, "$0.getMinutes()"),
        stdlib_fn!("Date", "second", [Type::Named("Date".to_string())], Type::Number, "$0.getSeconds()"),
        stdlib_fn!("Date", "millis", [Type::Named("Date".to_string())], Type::Number, "$0.getTime()"),
        stdlib_fn!("Date", "toIso", [Type::Named("Date".to_string())], Type::String, "$0.toISOString()"),
        // ── Http ──────────────────────────────────────────────────
        stdlib_fn!("Http", "get", [Type::String], result_of(Type::Named("Response".to_string()), Type::Named("Error".to_string())), "(async () => { try { const _r = await fetch($0); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "post", [Type::String, Type::Unknown], result_of(Type::Named("Response".to_string()), Type::Named("Error".to_string())), "(async () => { try { const _r = await fetch($0, { method: \"POST\", body: JSON.stringify($1), headers: { \"Content-Type\": \"application/json\" } }); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "put", [Type::String, Type::Unknown], result_of(Type::Named("Response".to_string()), Type::Named("Error".to_string())), "(async () => { try { const _r = await fetch($0, { method: \"PUT\", body: JSON.stringify($1), headers: { \"Content-Type\": \"application/json\" } }); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "delete", [Type::String], result_of(Type::Named("Response".to_string()), Type::Named("Error".to_string())), "(async () => { try { const _r = await fetch($0, { method: \"DELETE\" }); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "json", [Type::Named("Response".to_string())], result_of(Type::Unknown, Type::Named("Error".to_string())), "(async () => { try { const _r = await $0.json(); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "text", [Type::Named("Response".to_string())], result_of(Type::String, Type::Named("Error".to_string())), "(async () => { try { const _r = await $0.text(); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        // ── Promise ────────────────────────────────────────────────
        stdlib_fn!("Promise", "all", [array_of(promise_of(t.clone()))], promise_of(array_of(t.clone())), "Promise.all($0)"),
        stdlib_fn!("Promise", "race", [array_of(promise_of(t.clone()))], promise_of(t.clone()), "Promise.race($0)"),
        stdlib_fn!("Promise", "any", [array_of(promise_of(t.clone()))], promise_of(t.clone()), "Promise.any($0)"),
        stdlib_fn!("Promise", "allSettled", [array_of(promise_of(t.clone()))], promise_of(array_of(result_of(t.clone(), Type::Named("Error".to_string())))), "Promise.allSettled($0).then(_a => _a.map(_v => _v.status === \"fulfilled\" ? { ok: true as const, value: _v.value } : { ok: false as const, error: _v.reason instanceof Error ? _v.reason : new Error(String(_v.reason)) }))"),
        stdlib_fn!("Promise", "resolve", [t.clone()], promise_of(t.clone()), "Promise.resolve($0)"),
        stdlib_fn!("Promise", "reject", [u.clone()], promise_of(t.clone()), "Promise.reject($0)"),
        stdlib_fn!("Promise", "delay", [Type::Number], promise_of(Type::Unit), "new Promise(_r => setTimeout(_r, $0))"),
        // ── Pipe Utilities ────────────────────────────────────────
        stdlib_fn!("Pipe", "tap", [t.clone(), fun(vec![t.clone()], Type::Unit)], t.clone(), "(() => { const _v = $0; ($1)(_v); return _v; })()"),
        // ── JSON ───────────────────────────────────────────────
        stdlib_fn!("JSON", "stringify", [t.clone()], Type::String, "JSON.stringify($0)"),
        stdlib_fn!("JSON", "parse", [Type::String], result_of(t.clone(), Type::Named("ParseError".to_string())), "(() => { try { return { ok: true as const, value: JSON.parse($0) }; } catch (e) { return { ok: false as const, error: { message: String(e) } }; } })()"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_array_sort() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "sort").unwrap();
        assert_eq!(f.codegen, "[...$0].sort((a, b) => a - b)");
    }

    #[test]
    fn lookup_option_map() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "map").unwrap();
        assert!(f.codegen.contains("undefined"));
    }

    #[test]
    fn lookup_array_any() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "any").unwrap();
        assert_eq!(f.codegen, "$0.some($1)");
    }

    #[test]
    fn lookup_array_all() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "all").unwrap();
        assert_eq!(f.codegen, "$0.every($1)");
    }

    #[test]
    fn lookup_array_sum() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "sum").unwrap();
        assert_eq!(f.codegen, "$0.reduce((a, b) => a + b, 0)");
    }

    #[test]
    fn lookup_array_join() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "join").unwrap();
        assert_eq!(f.codegen, "$0.join($1)");
    }

    #[test]
    fn lookup_array_is_empty() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "isEmpty").unwrap();
        assert_eq!(f.codegen, "$0.length === 0");
    }

    #[test]
    fn lookup_array_chunk() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "chunk").unwrap();
        assert!(f.codegen.contains("slice"));
    }

    #[test]
    fn lookup_array_unique() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "unique").unwrap();
        assert_eq!(f.codegen, "[...new Set($0)]");
    }

    #[test]
    fn lookup_array_group_by() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "groupBy").unwrap();
        assert_eq!(f.codegen, "Object.groupBy($0, $1)");
    }

    #[test]
    fn lookup_map_empty() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "empty").unwrap();
        assert_eq!(f.codegen, "new Map()");
    }

    #[test]
    fn lookup_map_from_array() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "fromArray").unwrap();
        assert_eq!(f.codegen, "new Map($0)");
    }

    #[test]
    fn lookup_map_get() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "get").unwrap();
        assert_eq!(f.codegen, "$0.has($1) ? $0.get($1) : undefined");
    }

    #[test]
    fn lookup_map_set() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "set").unwrap();
        assert_eq!(f.codegen, "new Map([...$0, [$1, $2]])");
    }

    #[test]
    fn lookup_map_remove() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "remove").unwrap();
        assert!(f.codegen.contains("filter"));
    }

    #[test]
    fn lookup_map_has() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "has").unwrap();
        assert_eq!(f.codegen, "$0.has($1)");
    }

    #[test]
    fn lookup_map_keys() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "keys").unwrap();
        assert_eq!(f.codegen, "[...$0.keys()]");
    }

    #[test]
    fn lookup_map_values() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "values").unwrap();
        assert_eq!(f.codegen, "[...$0.values()]");
    }

    #[test]
    fn lookup_map_entries() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "entries").unwrap();
        assert_eq!(f.codegen, "[...$0.entries()]");
    }

    #[test]
    fn lookup_map_size() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "size").unwrap();
        assert_eq!(f.codegen, "$0.size");
    }

    #[test]
    fn lookup_map_is_empty() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "isEmpty").unwrap();
        assert_eq!(f.codegen, "$0.size === 0");
    }

    #[test]
    fn lookup_map_merge() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Map", "merge").unwrap();
        assert_eq!(f.codegen, "new Map([...$0, ...$1])");
    }

    #[test]
    fn lookup_set_empty() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "empty").unwrap();
        assert_eq!(f.codegen, "new Set()");
    }

    #[test]
    fn lookup_set_from_array() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "fromArray").unwrap();
        assert_eq!(f.codegen, "new Set($0)");
    }

    #[test]
    fn lookup_set_to_array() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "toArray").unwrap();
        assert_eq!(f.codegen, "[...$0]");
    }

    #[test]
    fn lookup_set_add() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "add").unwrap();
        assert_eq!(f.codegen, "new Set([...$0, $1])");
    }

    #[test]
    fn lookup_set_remove() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "remove").unwrap();
        assert!(f.codegen.contains("filter"));
    }

    #[test]
    fn lookup_set_has() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "has").unwrap();
        assert_eq!(f.codegen, "$0.has($1)");
    }

    #[test]
    fn lookup_set_size() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "size").unwrap();
        assert_eq!(f.codegen, "$0.size");
    }

    #[test]
    fn lookup_set_is_empty() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "isEmpty").unwrap();
        assert_eq!(f.codegen, "$0.size === 0");
    }

    #[test]
    fn lookup_set_union() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "union").unwrap();
        assert_eq!(f.codegen, "new Set([...$0, ...$1])");
    }

    #[test]
    fn lookup_set_intersect() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "intersect").unwrap();
        assert!(f.codegen.contains("filter"));
        assert!(f.codegen.contains("has"));
    }

    #[test]
    fn lookup_set_diff() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Set", "diff").unwrap();
        assert!(f.codegen.contains("filter"));
        assert!(f.codegen.contains("!$1.has"));
    }

    #[test]
    fn lookup_http_get() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Http", "get").unwrap();
        assert!(f.codegen.contains("fetch($0)"));
        assert!(f.codegen.contains("async"));
    }

    #[test]
    fn lookup_http_post() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Http", "post").unwrap();
        assert!(f.codegen.contains("POST"));
        assert!(f.codegen.contains("JSON.stringify($1)"));
    }

    #[test]
    fn lookup_http_put() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Http", "put").unwrap();
        assert!(f.codegen.contains("PUT"));
        assert!(f.codegen.contains("JSON.stringify($1)"));
    }

    #[test]
    fn lookup_http_delete() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Http", "delete").unwrap();
        assert!(f.codegen.contains("DELETE"));
    }

    #[test]
    fn lookup_http_json() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Http", "json").unwrap();
        assert!(f.codegen.contains("$0.json()"));
    }

    #[test]
    fn lookup_http_text() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Http", "text").unwrap();
        assert!(f.codegen.contains("$0.text()"));
    }

    #[test]
    fn lookup_nonexistent() {
        let reg = StdlibRegistry::new();
        assert!(reg.lookup("Array", "nonexistent").is_none());
        assert!(reg.lookup("Nonexistent", "sort").is_none());
    }

    #[test]
    fn is_module() {
        let reg = StdlibRegistry::new();
        assert!(reg.is_module("Array"));
        assert!(reg.is_module("Option"));
        assert!(reg.is_module("Result"));
        assert!(reg.is_module("String"));
        assert!(reg.is_module("Number"));
        assert!(reg.is_module("Console"));
        assert!(reg.is_module("Math"));
        assert!(reg.is_module("JSON"));
        assert!(reg.is_module("Pipe"));
        assert!(reg.is_module("Map"));
        assert!(reg.is_module("Set"));
        assert!(reg.is_module("Http"));
        assert!(reg.is_module("Date"));
        assert!(reg.is_module("Promise"));
        assert!(!reg.is_module("Foo"));
    }

    #[test]
    fn module_functions_count() {
        let reg = StdlibRegistry::new();
        assert!(reg.module_functions("Array").len() >= 16);
        assert!(reg.module_functions("Option").len() >= 13);
        assert!(reg.module_functions("Result").len() >= 16);
        assert!(reg.module_functions("Promise").len() >= 7);
        assert!(reg.module_functions("String").len() >= 10);
        assert!(reg.module_functions("Number").len() >= 5);
        assert!(reg.module_functions("Console").len() >= 5);
        assert!(reg.module_functions("Math").len() >= 15);
        assert!(reg.module_functions("JSON").len() >= 2);
        assert!(reg.module_functions("Map").len() >= 12);
        assert!(reg.module_functions("Set").len() >= 11);
        assert!(reg.module_functions("Http").len() >= 6);
        assert!(reg.module_functions("Date").len() >= 11);
    }

    #[test]
    fn lookup_date_now() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Date", "now").unwrap();
        assert_eq!(f.codegen, "new Date()");
        assert!(f.params.is_empty());
    }

    #[test]
    fn lookup_date_from() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Date", "from").unwrap();
        assert_eq!(f.codegen, "new Date($0)");
    }

    #[test]
    fn lookup_date_from_millis() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Date", "fromMillis").unwrap();
        assert_eq!(f.codegen, "new Date($0)");
    }

    #[test]
    fn lookup_date_year() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Date", "year").unwrap();
        assert_eq!(f.codegen, "$0.getFullYear()");
    }

    #[test]
    fn lookup_date_month() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Date", "month").unwrap();
        assert_eq!(f.codegen, "($0.getMonth() + 1)");
    }

    #[test]
    fn lookup_date_millis() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Date", "millis").unwrap();
        assert_eq!(f.codegen, "$0.getTime()");
    }

    #[test]
    fn lookup_date_to_iso() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Date", "toIso").unwrap();
        assert_eq!(f.codegen, "$0.toISOString()");
    }

    #[test]
    fn lookup_console_log() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Console", "log").unwrap();
        assert_eq!(f.codegen, "console.log($..)");
        assert!(f.is_variadic());
    }

    #[test]
    fn lookup_math_floor() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Math", "floor").unwrap();
        assert_eq!(f.codegen, "Math.floor($0)");
    }

    #[test]
    fn lookup_math_random() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Math", "random").unwrap();
        assert_eq!(f.codegen, "Math.random()");
        assert!(f.params.is_empty());
    }

    #[test]
    fn lookup_pipe_tap() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Pipe", "tap").unwrap();
        assert!(f.codegen.contains("return _v"));
    }

    #[test]
    fn lookup_json_stringify() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("JSON", "stringify").unwrap();
        assert_eq!(f.codegen, "JSON.stringify($0)");
    }

    // ── Option new functions ──────────────────────────────────

    #[test]
    fn lookup_option_filter() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "filter").unwrap();
        assert!(f.codegen.contains("undefined"));
        assert!(f.codegen.contains("($1)($0)"));
    }

    #[test]
    fn lookup_option_unwrap() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "unwrap").unwrap();
        assert!(f.codegen.contains("throw"));
    }

    #[test]
    fn lookup_option_map_or() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "mapOr").unwrap();
        assert!(f.codegen.contains("($2)($0)"));
        assert!(f.codegen.contains("$1"));
    }

    #[test]
    fn lookup_option_flatten() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "flatten").unwrap();
        assert_eq!(f.codegen, "$0");
    }

    #[test]
    fn lookup_option_zip() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "zip").unwrap();
        assert!(f.codegen.contains("[$0, $1]"));
    }

    #[test]
    fn lookup_option_inspect() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "inspect").unwrap();
        assert!(f.codegen.contains("($1)(_v)"));
        assert!(f.codegen.contains("return _v"));
    }

    #[test]
    fn lookup_option_to_err() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "toErr").unwrap();
        assert!(f.codegen.contains("ok: false"));
        assert!(f.codegen.contains("ok: true"));
    }

    #[test]
    fn lookup_option_all() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "all").unwrap();
        assert!(f.codegen.contains("for"));
        assert!(f.codegen.contains("undefined"));
    }

    #[test]
    fn lookup_option_any() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "any").unwrap();
        assert!(f.codegen.contains("find"));
    }

    // ── Result new functions ──────────────────────────────────

    #[test]
    fn lookup_result_filter() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "filter").unwrap();
        assert!(f.codegen.contains("($1)($0.value)"));
    }

    #[test]
    fn lookup_result_unwrap() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "unwrap").unwrap();
        assert!(f.codegen.contains("throw"));
        assert!(f.codegen.contains("$0.error"));
    }

    #[test]
    fn lookup_result_unwrap_err() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "unwrapErr").unwrap();
        assert!(f.codegen.contains("throw"));
        assert!(f.codegen.contains("$0.error"));
    }

    #[test]
    fn lookup_result_map_or() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "mapOr").unwrap();
        assert!(f.codegen.contains("($2)($0.value)"));
    }

    #[test]
    fn lookup_result_flatten() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "flatten").unwrap();
        assert!(f.codegen.contains("$0.value"));
    }

    #[test]
    fn lookup_result_zip() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "zip").unwrap();
        assert!(f.codegen.contains("$0.value, $1.value"));
    }

    #[test]
    fn lookup_result_inspect() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "inspect").unwrap();
        assert!(f.codegen.contains("_v.ok"));
        assert!(f.codegen.contains("($1)(_v.value)"));
    }

    #[test]
    fn lookup_result_inspect_err() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "inspectErr").unwrap();
        assert!(f.codegen.contains("!_v.ok"));
        assert!(f.codegen.contains("($1)(_v.error)"));
    }

    #[test]
    fn lookup_result_all() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "all").unwrap();
        assert!(f.codegen.contains("for"));
        assert!(f.codegen.contains("!_v.ok"));
    }

    #[test]
    fn lookup_result_any() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "any").unwrap();
        assert!(f.codegen.contains("for"));
        assert!(f.codegen.contains("_v.ok"));
    }

    // ── Array new functions ───────────────────────────────────

    #[test]
    fn lookup_array_map_result() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "mapResult").unwrap();
        assert!(f.codegen.contains("for"));
        assert!(f.codegen.contains("!_res.ok"));
    }

    // ── Promise functions ─────────────────────────────────────

    #[test]
    fn lookup_promise_all() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Promise", "all").unwrap();
        assert_eq!(f.codegen, "Promise.all($0)");
    }

    #[test]
    fn lookup_promise_race() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Promise", "race").unwrap();
        assert_eq!(f.codegen, "Promise.race($0)");
    }

    #[test]
    fn lookup_promise_any() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Promise", "any").unwrap();
        assert_eq!(f.codegen, "Promise.any($0)");
    }

    #[test]
    fn lookup_promise_all_settled() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Promise", "allSettled").unwrap();
        assert!(f.codegen.contains("allSettled"));
        assert!(f.codegen.contains("ok: true"));
        assert!(f.codegen.contains("ok: false"));
    }

    #[test]
    fn lookup_promise_resolve() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Promise", "resolve").unwrap();
        assert_eq!(f.codegen, "Promise.resolve($0)");
    }

    #[test]
    fn lookup_promise_reject() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Promise", "reject").unwrap();
        assert_eq!(f.codegen, "Promise.reject($0)");
    }

    #[test]
    fn lookup_promise_delay() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Promise", "delay").unwrap();
        assert!(f.codegen.contains("setTimeout"));
    }

    // ── Structural validation ─────────────────────────────────

    #[test]
    fn all_functions_have_valid_codegen_placeholders() {
        let reg = StdlibRegistry::new();
        for f in reg.all_functions() {
            if f.is_variadic() {
                assert!(
                    f.codegen.contains("$.."),
                    "{}.{} is variadic but codegen has no $.. placeholder",
                    f.module,
                    f.name
                );
            } else if !f.params.is_empty() {
                assert!(
                    f.codegen.contains("$0"),
                    "{}.{} has {} params but codegen has no $0 placeholder",
                    f.module,
                    f.name,
                    f.params.len()
                );
            }
        }
    }

    #[test]
    fn no_duplicate_function_registrations() {
        let reg = StdlibRegistry::new();
        let mut seen = std::collections::HashSet::new();
        for f in reg.all_functions() {
            let key = format!("{}.{}", f.module, f.name);
            assert!(
                seen.insert(key.clone()),
                "duplicate stdlib registration: {key}"
            );
        }
    }

    #[test]
    fn every_function_is_findable_by_lookup() {
        let reg = StdlibRegistry::new();
        for f in reg.all_functions() {
            assert!(
                reg.lookup(f.module, f.name).is_some(),
                "lookup failed for {}.{}",
                f.module,
                f.name
            );
        }
    }

    #[test]
    fn lookup_by_name_returns_all_overloads() {
        let reg = StdlibRegistry::new();
        // "map" exists in Array, Option, and Result
        let maps = reg.lookup_by_name("map");
        assert!(
            maps.len() >= 3,
            "expected map in at least 3 modules, found {}",
            maps.len()
        );
    }

    // ── Array (remaining) ─────────────────────────────────────

    #[test]
    fn lookup_array_map() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "map").unwrap();
        assert_eq!(f.codegen, "$0.map($1)");
    }

    #[test]
    fn lookup_array_filter() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "filter").unwrap();
        assert_eq!(f.codegen, "$0.filter($1)");
    }

    #[test]
    fn lookup_array_find() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "find").unwrap();
        assert_eq!(f.codegen, "$0.find($1)");
    }

    #[test]
    fn lookup_array_find_index() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "findIndex").unwrap();
        assert!(f.codegen.contains("findIndex"));
    }

    #[test]
    fn lookup_array_flat_map() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "flatMap").unwrap();
        assert_eq!(f.codegen, "$0.flatMap($1)");
    }

    #[test]
    fn lookup_array_at() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "at").unwrap();
        assert_eq!(f.codegen, "$0[$1]");
    }

    #[test]
    fn lookup_array_contains() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "contains").unwrap();
        assert!(f.codegen.contains("__floeEq"));
    }

    #[test]
    fn lookup_array_head() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "head").unwrap();
        assert_eq!(f.codegen, "$0[0]");
    }

    #[test]
    fn lookup_array_last() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "last").unwrap();
        assert!(f.codegen.contains("length - 1"));
    }

    #[test]
    fn lookup_array_take() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "take").unwrap();
        assert_eq!(f.codegen, "$0.slice(0, $1)");
    }

    #[test]
    fn lookup_array_drop() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "drop").unwrap();
        assert_eq!(f.codegen, "$0.slice($1)");
    }

    #[test]
    fn lookup_array_reverse() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "reverse").unwrap();
        assert_eq!(f.codegen, "[...$0].reverse()");
    }

    #[test]
    fn lookup_array_reduce() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "reduce").unwrap();
        assert_eq!(f.codegen, "$0.reduce($1, $2)");
    }

    #[test]
    fn lookup_array_length() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "length").unwrap();
        assert_eq!(f.codegen, "$0.length");
    }

    #[test]
    fn lookup_array_concat() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "concat").unwrap();
        assert_eq!(f.codegen, "[...$0, ...$1]");
    }

    #[test]
    fn lookup_array_append() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "append").unwrap();
        assert_eq!(f.codegen, "[...$0, $1]");
    }

    #[test]
    fn lookup_array_prepend() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "prepend").unwrap();
        assert_eq!(f.codegen, "[$1, ...$0]");
    }

    #[test]
    fn lookup_array_sort_by() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "sortBy").unwrap();
        assert!(f.codegen.contains("sort"));
        assert!(f.codegen.contains("($1)"));
    }

    #[test]
    fn lookup_array_zip() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "zip").unwrap();
        assert!(f.codegen.contains("as const"));
    }

    #[test]
    fn lookup_array_from() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Array", "from").unwrap();
        assert_eq!(f.codegen, "Array.from($0, $1)");
    }

    // ── Option (remaining) ────────────────────────────────────

    #[test]
    fn lookup_option_flat_map() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "flatMap").unwrap();
        assert!(f.codegen.contains("undefined"));
        assert!(f.codegen.contains("($1)($0)"));
    }

    #[test]
    fn lookup_option_unwrap_or() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "unwrapOr").unwrap();
        assert_eq!(f.codegen, "$0 !== undefined ? $0 : $1");
    }

    #[test]
    fn lookup_option_is_some() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "isSome").unwrap();
        assert_eq!(f.codegen, "$0 !== undefined");
    }

    #[test]
    fn lookup_option_is_none() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "isNone").unwrap();
        assert_eq!(f.codegen, "$0 === undefined");
    }

    #[test]
    fn lookup_option_to_result() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Option", "toResult").unwrap();
        assert!(f.codegen.contains("ok: true"));
        assert!(f.codegen.contains("ok: false"));
    }

    // ── Result (remaining) ────────────────────────────────────

    #[test]
    fn lookup_result_map() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "map").unwrap();
        assert!(f.codegen.contains("($1)($0.value)"));
    }

    #[test]
    fn lookup_result_map_err() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "mapErr").unwrap();
        assert!(f.codegen.contains("($1)($0.error)"));
    }

    #[test]
    fn lookup_result_flat_map() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "flatMap").unwrap();
        assert!(f.codegen.contains("($1)($0.value)"));
    }

    #[test]
    fn lookup_result_unwrap_or() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "unwrapOr").unwrap();
        assert!(f.codegen.contains("$0.value"));
        assert!(f.codegen.contains("$1"));
    }

    #[test]
    fn lookup_result_is_ok() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "isOk").unwrap();
        assert_eq!(f.codegen, "$0.ok");
    }

    #[test]
    fn lookup_result_is_err() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "isErr").unwrap();
        assert_eq!(f.codegen, "!$0.ok");
    }

    #[test]
    fn lookup_result_to_option() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Result", "toOption").unwrap();
        assert!(f.codegen.contains("$0.value"));
        assert!(f.codegen.contains("undefined"));
    }

    // ── String ────────────────────────────────────────────────

    #[test]
    fn lookup_string_trim() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "trim").unwrap();
        assert_eq!(f.codegen, "$0.trim()");
    }

    #[test]
    fn lookup_string_trim_start() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "trimStart").unwrap();
        assert_eq!(f.codegen, "$0.trimStart()");
    }

    #[test]
    fn lookup_string_trim_end() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "trimEnd").unwrap();
        assert_eq!(f.codegen, "$0.trimEnd()");
    }

    #[test]
    fn lookup_string_split() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "split").unwrap();
        assert_eq!(f.codegen, "$0.split($1)");
    }

    #[test]
    fn lookup_string_replace() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "replace").unwrap();
        assert_eq!(f.codegen, "$0.replace($1, $2)");
    }

    #[test]
    fn lookup_string_starts_with() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "startsWith").unwrap();
        assert_eq!(f.codegen, "$0.startsWith($1)");
    }

    #[test]
    fn lookup_string_ends_with() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "endsWith").unwrap();
        assert_eq!(f.codegen, "$0.endsWith($1)");
    }

    #[test]
    fn lookup_string_contains() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "contains").unwrap();
        assert_eq!(f.codegen, "$0.includes($1)");
    }

    #[test]
    fn lookup_string_to_upper_case() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "toUpperCase").unwrap();
        assert_eq!(f.codegen, "$0.toUpperCase()");
    }

    #[test]
    fn lookup_string_to_lower_case() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "toLowerCase").unwrap();
        assert_eq!(f.codegen, "$0.toLowerCase()");
    }

    #[test]
    fn lookup_string_length() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "length").unwrap();
        assert_eq!(f.codegen, "$0.length");
    }

    #[test]
    fn lookup_string_slice() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "slice").unwrap();
        assert_eq!(f.codegen, "$0.slice($1, $2)");
    }

    #[test]
    fn lookup_string_pad_start() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "padStart").unwrap();
        assert_eq!(f.codegen, "$0.padStart($1, $2)");
    }

    #[test]
    fn lookup_string_pad_end() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "padEnd").unwrap();
        assert_eq!(f.codegen, "$0.padEnd($1, $2)");
    }

    #[test]
    fn lookup_string_repeat() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "repeat").unwrap();
        assert_eq!(f.codegen, "$0.repeat($1)");
    }

    #[test]
    fn lookup_string_locale_compare() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("String", "localeCompare").unwrap();
        assert_eq!(f.codegen, "$0.localeCompare($1)");
    }

    // ── Number ────────────────────────────────────────────────

    #[test]
    fn lookup_number_parse() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Number", "parse").unwrap();
        assert!(f.codegen.contains("Number($0)"));
        assert!(f.codegen.contains("isNaN"));
    }

    #[test]
    fn lookup_number_clamp() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Number", "clamp").unwrap();
        assert_eq!(f.codegen, "Math.min(Math.max($0, $1), $2)");
    }

    #[test]
    fn lookup_number_is_finite() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Number", "isFinite").unwrap();
        assert_eq!(f.codegen, "Number.isFinite($0)");
    }

    #[test]
    fn lookup_number_is_integer() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Number", "isInteger").unwrap();
        assert_eq!(f.codegen, "Number.isInteger($0)");
    }

    #[test]
    fn lookup_number_to_fixed() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Number", "toFixed").unwrap();
        assert_eq!(f.codegen, "$0.toFixed($1)");
    }

    #[test]
    fn lookup_number_to_string() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Number", "toString").unwrap();
        assert_eq!(f.codegen, "String($0)");
    }

    // ── Console (remaining) ───────────────────────────────────

    #[test]
    fn lookup_console_warn() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Console", "warn").unwrap();
        assert_eq!(f.codegen, "console.warn($..)");
        assert!(f.is_variadic());
    }

    #[test]
    fn lookup_console_error() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Console", "error").unwrap();
        assert_eq!(f.codegen, "console.error($..)");
    }

    #[test]
    fn lookup_console_info() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Console", "info").unwrap();
        assert_eq!(f.codegen, "console.info($..)");
    }

    #[test]
    fn lookup_console_debug() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Console", "debug").unwrap();
        assert_eq!(f.codegen, "console.debug($..)");
    }

    #[test]
    fn lookup_console_time() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Console", "time").unwrap();
        assert_eq!(f.codegen, "console.time($0)");
        assert!(!f.is_variadic());
    }

    #[test]
    fn lookup_console_time_end() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Console", "timeEnd").unwrap();
        assert_eq!(f.codegen, "console.timeEnd($0)");
    }

    // ── Math (remaining) ──────────────────────────────────────

    #[test]
    fn lookup_math_ceil() {
        let reg = StdlibRegistry::new();
        assert_eq!(reg.lookup("Math", "ceil").unwrap().codegen, "Math.ceil($0)");
    }

    #[test]
    fn lookup_math_round() {
        let reg = StdlibRegistry::new();
        assert_eq!(
            reg.lookup("Math", "round").unwrap().codegen,
            "Math.round($0)"
        );
    }

    #[test]
    fn lookup_math_abs() {
        let reg = StdlibRegistry::new();
        assert_eq!(reg.lookup("Math", "abs").unwrap().codegen, "Math.abs($0)");
    }

    #[test]
    fn lookup_math_min() {
        let reg = StdlibRegistry::new();
        assert_eq!(
            reg.lookup("Math", "min").unwrap().codegen,
            "Math.min($0, $1)"
        );
    }

    #[test]
    fn lookup_math_max() {
        let reg = StdlibRegistry::new();
        assert_eq!(
            reg.lookup("Math", "max").unwrap().codegen,
            "Math.max($0, $1)"
        );
    }

    #[test]
    fn lookup_math_pow() {
        let reg = StdlibRegistry::new();
        assert_eq!(
            reg.lookup("Math", "pow").unwrap().codegen,
            "Math.pow($0, $1)"
        );
    }

    #[test]
    fn lookup_math_sqrt() {
        let reg = StdlibRegistry::new();
        assert_eq!(reg.lookup("Math", "sqrt").unwrap().codegen, "Math.sqrt($0)");
    }

    #[test]
    fn lookup_math_sign() {
        let reg = StdlibRegistry::new();
        assert_eq!(reg.lookup("Math", "sign").unwrap().codegen, "Math.sign($0)");
    }

    #[test]
    fn lookup_math_trunc() {
        let reg = StdlibRegistry::new();
        assert_eq!(
            reg.lookup("Math", "trunc").unwrap().codegen,
            "Math.trunc($0)"
        );
    }

    #[test]
    fn lookup_math_log() {
        let reg = StdlibRegistry::new();
        assert_eq!(reg.lookup("Math", "log").unwrap().codegen, "Math.log($0)");
    }

    #[test]
    fn lookup_math_trig() {
        let reg = StdlibRegistry::new();
        assert_eq!(reg.lookup("Math", "sin").unwrap().codegen, "Math.sin($0)");
        assert_eq!(reg.lookup("Math", "cos").unwrap().codegen, "Math.cos($0)");
        assert_eq!(reg.lookup("Math", "tan").unwrap().codegen, "Math.tan($0)");
    }

    // ── Date (remaining) ──────────────────────────────────────

    #[test]
    fn lookup_date_day() {
        let reg = StdlibRegistry::new();
        assert_eq!(reg.lookup("Date", "day").unwrap().codegen, "$0.getDate()");
    }

    #[test]
    fn lookup_date_hour() {
        let reg = StdlibRegistry::new();
        assert_eq!(reg.lookup("Date", "hour").unwrap().codegen, "$0.getHours()");
    }

    #[test]
    fn lookup_date_minute() {
        let reg = StdlibRegistry::new();
        assert_eq!(
            reg.lookup("Date", "minute").unwrap().codegen,
            "$0.getMinutes()"
        );
    }

    #[test]
    fn lookup_date_second() {
        let reg = StdlibRegistry::new();
        assert_eq!(
            reg.lookup("Date", "second").unwrap().codegen,
            "$0.getSeconds()"
        );
    }

    // ── JSON ──────────────────────────────────────────────────

    #[test]
    fn lookup_json_parse() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("JSON", "parse").unwrap();
        assert!(f.codegen.contains("JSON.parse($0)"));
        assert!(f.codegen.contains("ok: true"));
    }
}
