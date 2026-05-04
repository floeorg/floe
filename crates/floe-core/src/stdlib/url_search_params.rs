use super::{StdlibFn, Type, array_of, option_of, stdlib_fn};

/// URLSearchParams stdlib module — Floe-side read surface over the
/// runtime `URLSearchParams` (lib.dom.d.ts / Node).
///
/// `URLSearchParams.parse` doesn't return a Result because the runtime
/// constructor accepts arbitrary input — malformed pairs just become
/// empty entries, never throw. Mutation (`set`/`append`/`delete`) is
/// intentionally not exposed yet; callers either build from a tuple
/// array via `fromArray` or pull from a parsed URL.
#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let usp = || Type::Named("URLSearchParams".to_string());
    let pair = || Type::Tuple(vec![Type::String, Type::String]);

    fns.extend([
        stdlib_fn!(
            "URLSearchParams", "parse",
            [Type::String],
            usp(),
            "new URLSearchParams($0)"
        ),
        stdlib_fn!(
            "URLSearchParams", "fromArray",
            [array_of(pair())],
            usp(),
            "new URLSearchParams($0)"
        ),

        // Reads. `get` returns `Option<string>` because the runtime
        // returns `string | null` — coerce null to undefined so it lines
        // up with Floe's Option representation.
        stdlib_fn!(
            "URLSearchParams", "get",
            [usp(), Type::String],
            option_of(Type::String),
            "($0.get($1) ?? undefined)"
        ),
        stdlib_fn!(
            "URLSearchParams", "getAll",
            [usp(), Type::String],
            array_of(Type::String),
            "$0.getAll($1)"
        ),
        stdlib_fn!(
            "URLSearchParams", "has",
            [usp(), Type::String],
            Type::Bool,
            "$0.has($1)"
        ),

        stdlib_fn!(
            "URLSearchParams", "keys",
            [usp()],
            array_of(Type::String),
            "[...$0.keys()]"
        ),
        stdlib_fn!(
            "URLSearchParams", "values",
            [usp()],
            array_of(Type::String),
            "[...$0.values()]"
        ),
        stdlib_fn!(
            "URLSearchParams", "entries",
            [usp()],
            array_of(pair()),
            "[...$0.entries()]"
        ),

        stdlib_fn!(
            "URLSearchParams", "size",
            [usp()],
            Type::Number,
            "$0.size"
        ),
        stdlib_fn!(
            "URLSearchParams", "toString",
            [usp()],
            Type::String,
            "$0.toString()"
        ),
    ]);
}
