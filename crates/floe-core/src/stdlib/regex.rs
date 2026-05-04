use super::{StdlibFn, Type, array_of, option_of, result_of, stdlib_fn, try_catch_result};

/// RegExp stdlib module — Floe-side surface over the runtime `RegExp`
/// (lib.es5.d.ts). Compilation goes through `RegExp.compile(pattern,
/// flags) -> Result<RegExp, ParseError>` so invalid patterns surface as
/// Err rather than throwing. `flags` is a string like `"i"`, `"gm"`,
/// `""`; the JS constructor accepts an empty flags string.
#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let re = || Type::Named("RegExp".to_string());

    fns.extend([
        stdlib_fn!(
            "RegExp", "compile",
            [Type::String, Type::String],
            result_of(re(), Type::Named("ParseError".to_string())),
            try_catch_result!("new RegExp($0, $1)")
        ),

        // `test` short-circuits to a boolean.
        stdlib_fn!(
            "RegExp", "test",
            [re(), Type::String],
            Type::Bool,
            "$0.test($1)"
        ),

        // Returns the captures array for the first match, or `None`.
        // Named `exec` after the JS API (`RegExp.prototype.exec`) so the
        // receiver order maps cleanly to `r.exec(s)` without a swap, and
        // — incidentally — to avoid a name collision with Floe's `match`
        // keyword which would leak into bare-name pipe completion.
        stdlib_fn!(
            "RegExp", "exec",
            [re(), Type::String],
            option_of(array_of(Type::String)),
            "($0.exec($1) ?? undefined)"
        ),

        // Field accessors.
        stdlib_fn!("RegExp", "source", [re()], Type::String, "$0.source"),
        stdlib_fn!("RegExp", "flags",  [re()], Type::String, "$0.flags"),
    ]);
}
