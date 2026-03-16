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
    /// Example: `[...$0].sort((a, b) => a - b)` for Array.sort
    pub codegen: &'static str,
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
fn fun(params: Vec<Type>, ret: Type) -> Type {
    Type::Function {
        params,
        return_type: Box::new(ret),
    }
}

macro_rules! stdlib_fn {
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
        stdlib_fn!("Array", "contains", [array_of(t.clone()), t.clone()], Type::Bool, "$0.some((_item) => __zenEq(_item, $1))"),
        stdlib_fn!("Array", "head", [array_of(t.clone())], option_of(t.clone()), "$0[0]"),
        stdlib_fn!("Array", "last", [array_of(t.clone())], option_of(t.clone()), "$0[$0.length - 1]"),
        stdlib_fn!("Array", "take", [array_of(t.clone()), Type::Number], array_of(t.clone()), "$0.slice(0, $1)"),
        stdlib_fn!("Array", "drop", [array_of(t.clone()), Type::Number], array_of(t.clone()), "$0.slice($1)"),
        stdlib_fn!("Array", "reverse", [array_of(t.clone())], array_of(t.clone()), "[...$0].reverse()"),
        stdlib_fn!("Array", "reduce", [array_of(t.clone()), u.clone(), fun(vec![u.clone(), t.clone()], u.clone())], u.clone(), "$0.reduce($2, $1)"),
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
        // ── Option ──────────────────────────────────────────────
        stdlib_fn!("Option", "map", [option_of(t.clone()), fun(vec![t.clone()], u.clone())], option_of(u.clone()), "$0 !== undefined ? ($1)($0) : undefined"),
        stdlib_fn!("Option", "flatMap", [option_of(t.clone()), fun(vec![t.clone()], option_of(u.clone()))], option_of(u.clone()), "$0 !== undefined ? ($1)($0) : undefined"),
        stdlib_fn!("Option", "unwrapOr", [option_of(t.clone()), t.clone()], t.clone(), "$0 !== undefined ? $0 : $1"),
        stdlib_fn!("Option", "isSome", [option_of(t.clone())], Type::Bool, "$0 !== undefined"),
        stdlib_fn!("Option", "isNone", [option_of(t.clone())], Type::Bool, "$0 === undefined"),
        stdlib_fn!("Option", "toResult", [option_of(t.clone()), u.clone()], result_of(t.clone(), u.clone()), "$0 !== undefined ? { ok: true as const, value: $0 } : { ok: false as const, error: $1 }"),
        // ── Result ──────────────────────────────────────────────
        stdlib_fn!("Result", "map", [result_of(t.clone(), u.clone()), fun(vec![t.clone()], tv(2))], result_of(tv(2), u.clone()), "$0.ok ? { ok: true as const, value: ($1)($0.value) } : $0"),
        stdlib_fn!("Result", "mapErr", [result_of(t.clone(), u.clone()), fun(vec![u.clone()], tv(2))], result_of(t.clone(), tv(2)), "$0.ok ? $0 : { ok: false as const, error: ($1)($0.error) }"),
        stdlib_fn!("Result", "flatMap", [result_of(t.clone(), u.clone()), fun(vec![t.clone()], result_of(tv(2), u.clone()))], result_of(tv(2), u.clone()), "$0.ok ? ($1)($0.value) : $0"),
        stdlib_fn!("Result", "unwrapOr", [result_of(t.clone(), u.clone()), t.clone()], t.clone(), "$0.ok ? $0.value : $1"),
        stdlib_fn!("Result", "isOk", [result_of(t.clone(), u.clone())], Type::Bool, "$0.ok"),
        stdlib_fn!("Result", "isErr", [result_of(t.clone(), u.clone())], Type::Bool, "!$0.ok"),
        stdlib_fn!("Result", "toOption", [result_of(t.clone(), u.clone())], option_of(t.clone()), "$0.ok ? $0.value : undefined"),
        // ── String ──────────────────────────────────────────────
        stdlib_fn!("String", "trim", [Type::String], Type::String, "$0.trim()"),
        stdlib_fn!("String", "trimStart", [Type::String], Type::String, "$0.trimStart()"),
        stdlib_fn!("String", "trimEnd", [Type::String], Type::String, "$0.trimEnd()"),
        stdlib_fn!("String", "split", [Type::String, Type::String], array_of(Type::String), "$0.split($1)"),
        stdlib_fn!("String", "replace", [Type::String, Type::String, Type::String], Type::String, "$0.replace($1, $2)"),
        stdlib_fn!("String", "startsWith", [Type::String, Type::String], Type::Bool, "$0.startsWith($1)"),
        stdlib_fn!("String", "endsWith", [Type::String, Type::String], Type::Bool, "$0.endsWith($1)"),
        stdlib_fn!("String", "contains", [Type::String, Type::String], Type::Bool, "$0.includes($1)"),
        stdlib_fn!("String", "toUpper", [Type::String], Type::String, "$0.toUpperCase()"),
        stdlib_fn!("String", "toLower", [Type::String], Type::String, "$0.toLowerCase()"),
        stdlib_fn!("String", "length", [Type::String], Type::Number, "$0.length"),
        stdlib_fn!("String", "slice", [Type::String, Type::Number, Type::Number], Type::String, "$0.slice($1, $2)"),
        stdlib_fn!("String", "padStart", [Type::String, Type::Number, Type::String], Type::String, "$0.padStart($1, $2)"),
        stdlib_fn!("String", "padEnd", [Type::String, Type::Number, Type::String], Type::String, "$0.padEnd($1, $2)"),
        stdlib_fn!("String", "repeat", [Type::String, Type::Number], Type::String, "$0.repeat($1)"),
        // ── Number ──────────────────────────────────────────────
        stdlib_fn!("Number", "parse", [Type::String], result_of(Type::Number, Type::Named("ParseError".to_string())), "(() => { const _n = Number($0); return Number.isNaN(_n) || $0.trim() === \"\" ? { ok: false as const, error: { message: `Failed to parse \"${$0}\" as number` } } : { ok: true as const, value: _n }; })()"),
        stdlib_fn!("Number", "clamp", [Type::Number, Type::Number, Type::Number], Type::Number, "Math.min(Math.max($0, $1), $2)"),
        stdlib_fn!("Number", "isFinite", [Type::Number], Type::Bool, "Number.isFinite($0)"),
        stdlib_fn!("Number", "isInteger", [Type::Number], Type::Bool, "Number.isInteger($0)"),
        stdlib_fn!("Number", "toFixed", [Type::Number, Type::Number], Type::String, "$0.toFixed($1)"),
        stdlib_fn!("Number", "toString", [Type::Number], Type::String, "String($0)"),
        // ── Console ────────────────────────────────────────────
        stdlib_fn!("Console", "log", [t.clone()], Type::Unit, "console.log($0)"),
        stdlib_fn!("Console", "warn", [t.clone()], Type::Unit, "console.warn($0)"),
        stdlib_fn!("Console", "error", [t.clone()], Type::Unit, "console.error($0)"),
        stdlib_fn!("Console", "info", [t.clone()], Type::Unit, "console.info($0)"),
        stdlib_fn!("Console", "debug", [t.clone()], Type::Unit, "console.debug($0)"),
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
        assert!(!reg.is_module("Foo"));
    }

    #[test]
    fn module_functions_count() {
        let reg = StdlibRegistry::new();
        assert!(reg.module_functions("Array").len() >= 15);
        assert!(reg.module_functions("Option").len() >= 5);
        assert!(reg.module_functions("Result").len() >= 6);
        assert!(reg.module_functions("String").len() >= 10);
        assert!(reg.module_functions("Number").len() >= 5);
        assert!(reg.module_functions("Console").len() >= 5);
        assert!(reg.module_functions("Math").len() >= 14);
        assert!(reg.module_functions("JSON").len() >= 2);
    }

    #[test]
    fn lookup_console_log() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Console", "log").unwrap();
        assert_eq!(f.codegen, "console.log($0)");
    }

    #[test]
    fn lookup_math_floor() {
        let reg = StdlibRegistry::new();
        let f = reg.lookup("Math", "floor").unwrap();
        assert_eq!(f.codegen, "Math.floor($0)");
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
}
