use super::{StdlibFn, Type, result_of, stdlib_fn, try_catch_result};

/// URL stdlib module — Floe-side surface over the runtime `URL` type
/// (lib.dom.d.ts / Node URL). The Floe-side `URL` IS the runtime URL;
/// passing a value typed `URL` to any TS function expecting `URL` is a
/// zero-cost passthrough. Construction is `URL.parse(s) -> Result<URL,
/// ParseError>` rather than a throwing constructor so the error case is
/// surfaced in the type.
#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let url = || Type::Named("URL".to_string());

    fns.extend([
        // Construction: try/catch wrapper around `new URL` so invalid
        // inputs surface as `Err`, not a runtime throw.
        stdlib_fn!(
            "URL", "parse",
            [Type::String],
            result_of(url(), Type::Named("ParseError".to_string())),
            try_catch_result!("new URL($0)")
        ),

        // Field accessors. Match the runtime URL property names so users
        // who already know the JS API don't have to re-learn naming.
        stdlib_fn!("URL", "href",      [url()], Type::String, "$0.href"),
        stdlib_fn!("URL", "origin",    [url()], Type::String, "$0.origin"),
        stdlib_fn!("URL", "protocol",  [url()], Type::String, "$0.protocol"),
        stdlib_fn!("URL", "host",      [url()], Type::String, "$0.host"),
        stdlib_fn!("URL", "hostname",  [url()], Type::String, "$0.hostname"),
        stdlib_fn!("URL", "port",      [url()], Type::String, "$0.port"),
        stdlib_fn!("URL", "pathname",  [url()], Type::String, "$0.pathname"),
        stdlib_fn!("URL", "search",    [url()], Type::String, "$0.search"),
        stdlib_fn!("URL", "hash",      [url()], Type::String, "$0.hash"),

        // Parsed query params. Returns the live URLSearchParams view —
        // mutations on it would be reflected in the URL, but Floe's
        // URLSearchParams surface is read-only so this is safe.
        stdlib_fn!(
            "URL", "searchParams",
            [url()],
            Type::Named("URLSearchParams".to_string()),
            "$0.searchParams"
        ),

        stdlib_fn!("URL", "toString", [url()], Type::String, "$0.toString()"),
    ]);
}
