//! ZenScript standard library — built-in functions known to the compiler.
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

/// Build the full stdlib registry.
fn build_stdlib() -> Vec<StdlibFn> {
    let t = tv(0); // T
    let u = tv(1); // U

    vec![
        // ── Array ───────────────────────────────────────────────
        StdlibFn {
            module: "Array",
            name: "sort",
            params: vec![array_of(t.clone())],
            return_type: array_of(t.clone()),
            codegen: "[...$0].sort((a, b) => a - b)",
        },
        StdlibFn {
            module: "Array",
            name: "sortBy",
            params: vec![array_of(t.clone()), fun(vec![t.clone()], Type::Number)],
            return_type: array_of(t.clone()),
            codegen: "[...$0].sort((a, b) => ($1)(a) - ($1)(b))",
        },
        StdlibFn {
            module: "Array",
            name: "map",
            params: vec![array_of(t.clone()), fun(vec![t.clone()], u.clone())],
            return_type: array_of(u.clone()),
            codegen: "$0.map($1)",
        },
        StdlibFn {
            module: "Array",
            name: "filter",
            params: vec![array_of(t.clone()), fun(vec![t.clone()], Type::Bool)],
            return_type: array_of(t.clone()),
            codegen: "$0.filter($1)",
        },
        StdlibFn {
            module: "Array",
            name: "find",
            params: vec![array_of(t.clone()), fun(vec![t.clone()], Type::Bool)],
            return_type: option_of(t.clone()),
            codegen: "$0.find($1)",
        },
        StdlibFn {
            module: "Array",
            name: "findIndex",
            params: vec![array_of(t.clone()), fun(vec![t.clone()], Type::Bool)],
            return_type: option_of(Type::Number),
            codegen: "(() => { const _i = $0.findIndex($1); return _i === -1 ? undefined : _i; })()",
        },
        StdlibFn {
            module: "Array",
            name: "flatMap",
            params: vec![
                array_of(t.clone()),
                fun(vec![t.clone()], array_of(u.clone())),
            ],
            return_type: array_of(u.clone()),
            codegen: "$0.flatMap($1)",
        },
        StdlibFn {
            module: "Array",
            name: "at",
            params: vec![array_of(t.clone()), Type::Number],
            return_type: option_of(t.clone()),
            codegen: "$0[$1]",
        },
        StdlibFn {
            module: "Array",
            name: "contains",
            params: vec![array_of(t.clone()), t.clone()],
            return_type: Type::Bool,
            codegen: "$0.some((_item) => __zenEq(_item, $1))",
        },
        StdlibFn {
            module: "Array",
            name: "head",
            params: vec![array_of(t.clone())],
            return_type: option_of(t.clone()),
            codegen: "$0[0]",
        },
        StdlibFn {
            module: "Array",
            name: "last",
            params: vec![array_of(t.clone())],
            return_type: option_of(t.clone()),
            codegen: "$0[$0.length - 1]",
        },
        StdlibFn {
            module: "Array",
            name: "take",
            params: vec![array_of(t.clone()), Type::Number],
            return_type: array_of(t.clone()),
            codegen: "$0.slice(0, $1)",
        },
        StdlibFn {
            module: "Array",
            name: "drop",
            params: vec![array_of(t.clone()), Type::Number],
            return_type: array_of(t.clone()),
            codegen: "$0.slice($1)",
        },
        StdlibFn {
            module: "Array",
            name: "reverse",
            params: vec![array_of(t.clone())],
            return_type: array_of(t.clone()),
            codegen: "[...$0].reverse()",
        },
        StdlibFn {
            module: "Array",
            name: "reduce",
            params: vec![
                array_of(t.clone()),
                u.clone(),
                fun(vec![u.clone(), t.clone()], u.clone()),
            ],
            return_type: u.clone(),
            codegen: "$0.reduce($2, $1)",
        },
        StdlibFn {
            module: "Array",
            name: "length",
            params: vec![array_of(t.clone())],
            return_type: Type::Number,
            codegen: "$0.length",
        },
        StdlibFn {
            module: "Array",
            name: "zip",
            params: vec![array_of(t.clone()), array_of(u.clone())],
            return_type: array_of(Type::Tuple(vec![t.clone(), u.clone()])),
            codegen: "$0.map((_v, _i) => [_v, $1[_i]] as const)",
        },
        // ── Option ──────────────────────────────────────────────
        StdlibFn {
            module: "Option",
            name: "map",
            params: vec![option_of(t.clone()), fun(vec![t.clone()], u.clone())],
            return_type: option_of(u.clone()),
            codegen: "$0 !== undefined ? ($1)($0) : undefined",
        },
        StdlibFn {
            module: "Option",
            name: "flatMap",
            params: vec![
                option_of(t.clone()),
                fun(vec![t.clone()], option_of(u.clone())),
            ],
            return_type: option_of(u.clone()),
            codegen: "$0 !== undefined ? ($1)($0) : undefined",
        },
        StdlibFn {
            module: "Option",
            name: "unwrapOr",
            params: vec![option_of(t.clone()), t.clone()],
            return_type: t.clone(),
            codegen: "$0 !== undefined ? $0 : $1",
        },
        StdlibFn {
            module: "Option",
            name: "isSome",
            params: vec![option_of(t.clone())],
            return_type: Type::Bool,
            codegen: "$0 !== undefined",
        },
        StdlibFn {
            module: "Option",
            name: "isNone",
            params: vec![option_of(t.clone())],
            return_type: Type::Bool,
            codegen: "$0 === undefined",
        },
        StdlibFn {
            module: "Option",
            name: "toResult",
            params: vec![option_of(t.clone()), u.clone()],
            return_type: result_of(t.clone(), u.clone()),
            codegen: "$0 !== undefined ? { ok: true as const, value: $0 } : { ok: false as const, error: $1 }",
        },
        // ── Result ──────────────────────────────────────────────
        StdlibFn {
            module: "Result",
            name: "map",
            params: vec![result_of(t.clone(), u.clone()), fun(vec![t.clone()], tv(2))],
            return_type: result_of(tv(2), u.clone()),
            codegen: "$0.ok ? { ok: true as const, value: ($1)($0.value) } : $0",
        },
        StdlibFn {
            module: "Result",
            name: "mapErr",
            params: vec![result_of(t.clone(), u.clone()), fun(vec![u.clone()], tv(2))],
            return_type: result_of(t.clone(), tv(2)),
            codegen: "$0.ok ? $0 : { ok: false as const, error: ($1)($0.error) }",
        },
        StdlibFn {
            module: "Result",
            name: "flatMap",
            params: vec![
                result_of(t.clone(), u.clone()),
                fun(vec![t.clone()], result_of(tv(2), u.clone())),
            ],
            return_type: result_of(tv(2), u.clone()),
            codegen: "$0.ok ? ($1)($0.value) : $0",
        },
        StdlibFn {
            module: "Result",
            name: "unwrapOr",
            params: vec![result_of(t.clone(), u.clone()), t.clone()],
            return_type: t.clone(),
            codegen: "$0.ok ? $0.value : $1",
        },
        StdlibFn {
            module: "Result",
            name: "isOk",
            params: vec![result_of(t.clone(), u.clone())],
            return_type: Type::Bool,
            codegen: "$0.ok",
        },
        StdlibFn {
            module: "Result",
            name: "isErr",
            params: vec![result_of(t.clone(), u.clone())],
            return_type: Type::Bool,
            codegen: "!$0.ok",
        },
        StdlibFn {
            module: "Result",
            name: "toOption",
            params: vec![result_of(t.clone(), u.clone())],
            return_type: option_of(t.clone()),
            codegen: "$0.ok ? $0.value : undefined",
        },
        // ── String ──────────────────────────────────────────────
        StdlibFn {
            module: "String",
            name: "trim",
            params: vec![Type::String],
            return_type: Type::String,
            codegen: "$0.trim()",
        },
        StdlibFn {
            module: "String",
            name: "trimStart",
            params: vec![Type::String],
            return_type: Type::String,
            codegen: "$0.trimStart()",
        },
        StdlibFn {
            module: "String",
            name: "trimEnd",
            params: vec![Type::String],
            return_type: Type::String,
            codegen: "$0.trimEnd()",
        },
        StdlibFn {
            module: "String",
            name: "split",
            params: vec![Type::String, Type::String],
            return_type: array_of(Type::String),
            codegen: "$0.split($1)",
        },
        StdlibFn {
            module: "String",
            name: "replace",
            params: vec![Type::String, Type::String, Type::String],
            return_type: Type::String,
            codegen: "$0.replace($1, $2)",
        },
        StdlibFn {
            module: "String",
            name: "startsWith",
            params: vec![Type::String, Type::String],
            return_type: Type::Bool,
            codegen: "$0.startsWith($1)",
        },
        StdlibFn {
            module: "String",
            name: "endsWith",
            params: vec![Type::String, Type::String],
            return_type: Type::Bool,
            codegen: "$0.endsWith($1)",
        },
        StdlibFn {
            module: "String",
            name: "contains",
            params: vec![Type::String, Type::String],
            return_type: Type::Bool,
            codegen: "$0.includes($1)",
        },
        StdlibFn {
            module: "String",
            name: "toUpper",
            params: vec![Type::String],
            return_type: Type::String,
            codegen: "$0.toUpperCase()",
        },
        StdlibFn {
            module: "String",
            name: "toLower",
            params: vec![Type::String],
            return_type: Type::String,
            codegen: "$0.toLowerCase()",
        },
        StdlibFn {
            module: "String",
            name: "length",
            params: vec![Type::String],
            return_type: Type::Number,
            codegen: "$0.length",
        },
        StdlibFn {
            module: "String",
            name: "slice",
            params: vec![Type::String, Type::Number, Type::Number],
            return_type: Type::String,
            codegen: "$0.slice($1, $2)",
        },
        StdlibFn {
            module: "String",
            name: "padStart",
            params: vec![Type::String, Type::Number, Type::String],
            return_type: Type::String,
            codegen: "$0.padStart($1, $2)",
        },
        StdlibFn {
            module: "String",
            name: "padEnd",
            params: vec![Type::String, Type::Number, Type::String],
            return_type: Type::String,
            codegen: "$0.padEnd($1, $2)",
        },
        StdlibFn {
            module: "String",
            name: "repeat",
            params: vec![Type::String, Type::Number],
            return_type: Type::String,
            codegen: "$0.repeat($1)",
        },
        // ── Number ──────────────────────────────────────────────
        StdlibFn {
            module: "Number",
            name: "parse",
            params: vec![Type::String],
            return_type: result_of(Type::Number, Type::Named("ParseError".to_string())),
            codegen: "(() => { const _n = Number($0); return Number.isNaN(_n) || $0.trim() === \"\" ? { ok: false as const, error: { message: `Failed to parse \"${$0}\" as number` } } : { ok: true as const, value: _n }; })()",
        },
        StdlibFn {
            module: "Number",
            name: "clamp",
            params: vec![Type::Number, Type::Number, Type::Number],
            return_type: Type::Number,
            codegen: "Math.min(Math.max($0, $1), $2)",
        },
        StdlibFn {
            module: "Number",
            name: "isFinite",
            params: vec![Type::Number],
            return_type: Type::Bool,
            codegen: "Number.isFinite($0)",
        },
        StdlibFn {
            module: "Number",
            name: "isInteger",
            params: vec![Type::Number],
            return_type: Type::Bool,
            codegen: "Number.isInteger($0)",
        },
        StdlibFn {
            module: "Number",
            name: "toFixed",
            params: vec![Type::Number, Type::Number],
            return_type: Type::String,
            codegen: "$0.toFixed($1)",
        },
        StdlibFn {
            module: "Number",
            name: "toString",
            params: vec![Type::Number],
            return_type: Type::String,
            codegen: "String($0)",
        },
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
    }
}
