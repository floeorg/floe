use super::*;

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    fns.extend([
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
    ]);
}
