//! Codegen dispatcher.
//!
//! This module is a thin routing layer: it runs the collection pass to build
//! a `TypeContext`, then delegates emission to `typescript::TypeScriptGenerator`.
//! All actual TypeScript emission lives under `codegen/typescript/`, built on
//! the `pretty::Document` combinator.

mod typescript;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use crate::parser::ast::*;
use crate::resolve::ResolvedImports;

use typescript::generator::{TypeContext, TypeScriptGenerator};

/// Runtime deep-equality helper function name.
pub(crate) const DEEP_EQUAL_FN: &str = "__floeEq";

/// Produce a mangled name for a for-block function: `TypeName__funcName`.
/// Generic types are flattened: `Array<User>` → `Array_User`, `Map<string, number>` → `Map_string_number`.
pub fn for_block_fn_name<T>(type_expr: &TypeExpr<T>, fn_name: &str) -> String {
    let type_prefix = mangle_type_name(type_expr);
    format!("{type_prefix}__{fn_name}")
}

/// Mangle a type expression into a valid identifier fragment.
fn mangle_type_name<T>(type_expr: &TypeExpr<T>) -> String {
    match &type_expr.kind {
        TypeExprKind::Named {
            name, type_args, ..
        } => {
            if type_args.is_empty() {
                name.replace('.', "_")
            } else {
                let args: Vec<String> = type_args.iter().map(mangle_type_name).collect();
                format!("{}_{}", name.replace('.', "_"), args.join("_"))
            }
        }
        TypeExprKind::Array(inner) => format!("Array_{}", mangle_type_name(inner)),
        TypeExprKind::Tuple(parts) => {
            let parts: Vec<String> = parts.iter().map(mangle_type_name).collect();
            format!("Tuple_{}", parts.join("_"))
        }
        _ => unreachable!("for-block type must be Named, Array, or Tuple"),
    }
}

/// Code generation result: the emitted TypeScript source and whether it contains JSX.
pub struct CodegenOutput {
    pub code: String,
    pub has_jsx: bool,
    /// Declaration stub content for `.d.ts` files.
    pub dts: String,
}

/// The Floe code generator. Thin dispatcher that builds a `TypeContext` from
/// the program, then delegates to the `TypeScriptGenerator` built on `pretty::Document`.
pub struct Codegen {
    resolved_imports: HashMap<String, ResolvedImports>,
    test_mode: bool,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            resolved_imports: HashMap::new(),
            test_mode: false,
        }
    }

    /// Create a codegen with resolved import info.
    pub fn with_imports(resolved: &HashMap<String, ResolvedImports>) -> Self {
        Self {
            resolved_imports: resolved.clone(),
            test_mode: false,
        }
    }

    /// Enable test mode: test blocks will be emitted instead of stripped.
    pub fn with_test_mode(mut self) -> Self {
        self.test_mode = true;
        self
    }

    /// Generate TypeScript from a Floe program.
    pub fn generate(self, program: &TypedProgram) -> CodegenOutput {
        let ctx = TypeContext::from_program(program, &self.resolved_imports, self.test_mode);
        let mut generator = TypeScriptGenerator::new(&ctx);
        generator.generate(program)
    }
}

impl Default for Codegen {
    fn default() -> Self {
        Self::new()
    }
}

// ── Public Helpers ───────────────────────────────────────────────

/// Expand a codegen template like `$0.map($1)` with actual arg strings.
pub(crate) fn expand_codegen_template(template: &str, args: &[String]) -> String {
    let mut result = template.to_string();
    // Replace variadic placeholder ($..) with all args comma-separated
    result = result.replace("$..", &args.join(", "));
    // Replace in reverse order so $10 doesn't get matched by $1
    for (i, arg) in args.iter().enumerate().rev() {
        let placeholder = format!("${i}");
        // If the arg contains a ternary and the template uses it with member access
        // or call (e.g. `$0.filter(...)` or `$0[0]`), wrap in parens to avoid
        // the ternary's false-branch binding to the member access.
        let needs_parens = arg.contains(" ? ")
            && result
                .find(&placeholder)
                .and_then(|pos| result.as_bytes().get(pos + placeholder.len()))
                .is_some_and(|&ch| ch == b'.' || ch == b'[' || ch == b'(');
        let replacement = if needs_parens {
            format!("({arg})")
        } else {
            arg.clone()
        };
        result = result.replace(&placeholder, &replacement);
    }
    result
}

pub(crate) fn binop_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "===",
        BinOp::NotEq => "!==",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::LtEq => "<=",
        BinOp::GtEq => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

pub(crate) fn unaryop_str(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
    }
}

pub(crate) fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub(crate) fn has_placeholder_arg(args: &[TypedArg]) -> bool {
    args.iter().any(|a| match a {
        Arg::Positional(expr) => matches!(expr.kind, ExprKind::Placeholder),
        Arg::Named { value, .. } => matches!(value.kind, ExprKind::Placeholder),
    })
}

// ── Name Collection Passes ───────────────────────────────────────

/// Collect type names used as constructors (e.g. `User(name: "x")`).
pub(crate) fn collect_constructor_names(program: &TypedProgram) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &program.items {
        collect_constructors_from_item(item, &mut names);
    }
    names
}

fn collect_constructors_from_expr(expr: &TypedExpr, names: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Construct {
            type_name,
            args,
            spread,
            ..
        } => {
            names.insert(type_name.clone());
            if let Some(s) = spread {
                collect_constructors_from_expr(s, names);
            }
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        collect_constructors_from_expr(e, names);
                    }
                }
            }
        }
        ExprKind::Block(items) | ExprKind::Collect(items) => {
            for item in items {
                collect_constructors_from_item(item, names);
            }
        }
        ExprKind::Call { callee, args, .. } => {
            collect_constructors_from_expr(callee, names);
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        collect_constructors_from_expr(e, names);
                    }
                }
            }
        }
        ExprKind::Arrow { body, .. } => collect_constructors_from_expr(body, names),
        ExprKind::Pipe { left, right } | ExprKind::Binary { left, right, .. } => {
            collect_constructors_from_expr(left, names);
            collect_constructors_from_expr(right, names);
        }
        ExprKind::Match { subject, arms } => {
            collect_constructors_from_expr(subject, names);
            for arm in arms {
                collect_constructors_from_expr(&arm.body, names);
            }
        }
        ExprKind::Unwrap(e) | ExprKind::Value(e) | ExprKind::Unary { operand: e, .. } => {
            collect_constructors_from_expr(e, names);
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_constructors_from_expr(item, names);
            }
        }
        ExprKind::Jsx(jsx) => collect_constructors_from_jsx(jsx, names),
        _ => {}
    }
}

fn collect_constructors_from_jsx(jsx: &TypedJsxElement, names: &mut HashSet<String>) {
    match &jsx.kind {
        JsxElementKind::Element {
            props, children, ..
        } => {
            for prop in props {
                match prop {
                    JsxProp::Named { value: Some(v), .. } => {
                        collect_constructors_from_expr(v, names)
                    }
                    JsxProp::Spread { expr, .. } => collect_constructors_from_expr(expr, names),
                    _ => {}
                }
            }
            for child in children {
                match child {
                    JsxChild::Expr(e) => collect_constructors_from_expr(e, names),
                    JsxChild::Element(el) => collect_constructors_from_jsx(el, names),
                    _ => {}
                }
            }
        }
        JsxElementKind::Fragment { children } => {
            for child in children {
                match child {
                    JsxChild::Expr(e) => collect_constructors_from_expr(e, names),
                    JsxChild::Element(el) => collect_constructors_from_jsx(el, names),
                    _ => {}
                }
            }
        }
    }
}

fn collect_constructors_from_item(item: &TypedItem, names: &mut HashSet<String>) {
    match &item.kind {
        ItemKind::Const(decl) => collect_constructors_from_expr(&decl.value, names),
        ItemKind::Function(decl) => collect_constructors_from_expr(&decl.body, names),
        ItemKind::ForBlock(block) => {
            for func in &block.functions {
                collect_constructors_from_expr(&func.body, names);
            }
        }
        ItemKind::Expr(expr) => collect_constructors_from_expr(expr, names),
        _ => {}
    }
}

/// Collect all names used in value positions (expressions, not type annotations).
/// Used to detect type-only imports for `import type { ... }` codegen.
pub(crate) fn collect_value_used_names(program: &TypedProgram) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &program.items {
        collect_value_names_from_item(item, &mut names);
    }
    names
}

fn collect_value_names_from_expr(expr: &TypedExpr, names: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Identifier(name) => {
            names.insert(name.clone());
        }
        ExprKind::Construct {
            type_name,
            args,
            spread,
            ..
        } => {
            names.insert(type_name.clone());
            if let Some(s) = spread {
                collect_value_names_from_expr(s, names);
            }
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        collect_value_names_from_expr(e, names);
                    }
                }
            }
        }
        ExprKind::Call { callee, args, .. } => {
            collect_value_names_from_expr(callee, names);
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        collect_value_names_from_expr(e, names);
                    }
                }
            }
        }
        ExprKind::Block(items) | ExprKind::Collect(items) => {
            for item in items {
                collect_value_names_from_item(item, names);
            }
        }
        ExprKind::Arrow { body, .. } => collect_value_names_from_expr(body, names),
        ExprKind::Member { object, .. } => collect_value_names_from_expr(object, names),
        ExprKind::Pipe { left, right } | ExprKind::Binary { left, right, .. } => {
            collect_value_names_from_expr(left, names);
            collect_value_names_from_expr(right, names);
        }
        ExprKind::Unary { operand, .. } => collect_value_names_from_expr(operand, names),
        ExprKind::Unwrap(e) => {
            collect_value_names_from_expr(e, names);
        }
        ExprKind::Value(e) => {
            collect_value_names_from_expr(e, names);
        }
        ExprKind::Match { subject, arms } => {
            collect_value_names_from_expr(subject, names);
            for arm in arms {
                collect_value_names_from_expr(&arm.body, names);
                if let Some(guard) = &arm.guard {
                    collect_value_names_from_expr(guard, names);
                }
            }
        }
        ExprKind::Index { object, index } => {
            collect_value_names_from_expr(object, names);
            collect_value_names_from_expr(index, names);
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_value_names_from_expr(item, names);
            }
        }
        ExprKind::TemplateLiteral(parts) => {
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    collect_value_names_from_expr(e, names);
                }
            }
        }
        ExprKind::TaggedTemplate { tag, parts } => {
            collect_value_names_from_expr(tag, names);
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    collect_value_names_from_expr(e, names);
                }
            }
        }
        ExprKind::Object(fields) => {
            for (_, value) in fields {
                collect_value_names_from_expr(value, names);
            }
        }
        ExprKind::Grouped(inner) | ExprKind::Spread(inner) => {
            collect_value_names_from_expr(inner, names);
        }
        ExprKind::Jsx(jsx) => collect_value_names_from_jsx(jsx, names),
        _ => {}
    }
}

fn collect_value_names_from_jsx(jsx: &TypedJsxElement, names: &mut HashSet<String>) {
    match &jsx.kind {
        JsxElementKind::Element {
            name,
            props,
            children,
            ..
        } => {
            // Uppercase component names are value references (e.g. <QueryClientProvider>)
            if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                names.insert(name.clone());
            }
            for prop in props {
                match prop {
                    JsxProp::Named { value, .. } => {
                        if let Some(value) = value {
                            collect_value_names_from_expr(value, names);
                        }
                    }
                    JsxProp::Spread { expr, .. } => {
                        collect_value_names_from_expr(expr, names);
                    }
                }
            }
            for child in children {
                match child {
                    JsxChild::Expr(e) => collect_value_names_from_expr(e, names),
                    JsxChild::Element(el) => collect_value_names_from_jsx(el, names),
                    JsxChild::Text(_) => {}
                }
            }
        }
        JsxElementKind::Fragment { children } => {
            for child in children {
                match child {
                    JsxChild::Expr(e) => collect_value_names_from_expr(e, names),
                    JsxChild::Element(el) => collect_value_names_from_jsx(el, names),
                    JsxChild::Text(_) => {}
                }
            }
        }
    }
}

fn collect_value_names_from_item(item: &TypedItem, names: &mut HashSet<String>) {
    match &item.kind {
        ItemKind::Const(decl) => collect_value_names_from_expr(&decl.value, names),
        ItemKind::Function(decl) => collect_value_names_from_expr(&decl.body, names),
        ItemKind::ForBlock(block) => {
            for func in &block.functions {
                collect_value_names_from_expr(&func.body, names);
            }
        }
        ItemKind::Expr(expr) => collect_value_names_from_expr(expr, names),
        _ => {}
    }
}
