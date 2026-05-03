use super::{StdlibFn, Type, stdlib_fn};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    fns.extend([
        stdlib_fn!("Console", "log", Type::Unit, "console.log($..)"),
        stdlib_fn!("Console", "warn", Type::Unit, "console.warn($..)"),
        stdlib_fn!("Console", "error", Type::Unit, "console.error($..)"),
        stdlib_fn!("Console", "info", Type::Unit, "console.info($..)"),
        stdlib_fn!("Console", "debug", Type::Unit, "console.debug($..)"),
        stdlib_fn!("Console", "time", [Type::String], Type::Unit, "console.time($0)"),
        stdlib_fn!("Console", "timeEnd", [Type::String], Type::Unit, "console.timeEnd($0)"),
    ]);
}
