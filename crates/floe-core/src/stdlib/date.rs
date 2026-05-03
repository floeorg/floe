use super::{StdlibFn, Type, stdlib_fn};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    fns.extend([
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
    ]);
}
