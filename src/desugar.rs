//! Desugar pass: transforms high-level AST constructs into simpler equivalents.
//!
//! Runs after the checker and before codegen. Each transform replaces a
//! language-level construct with lower-level AST nodes that codegen can
//! emit without needing semantic knowledge.
//!
//! Current transforms:
//! - `Some(x)` → `x` (identity — Option is `T | undefined`)
//! - `None`   → `Identifier("undefined")`

use crate::parser::ast::*;

/// Run the desugar pass over a program, transforming it in place.
pub fn desugar_program(program: &mut Program) {
    for item in &mut program.items {
        desugar_item(item);
    }
}

fn desugar_item(item: &mut Item) {
    match &mut item.kind {
        ItemKind::Expr(expr) => desugar_expr(expr),
        ItemKind::Const(decl) => {
            desugar_expr(&mut decl.value);
        }
        ItemKind::Function(decl) => {
            desugar_function(decl);
        }
        ItemKind::ForBlock(block) => {
            for func in &mut block.functions {
                desugar_function(func);
            }
        }
        ItemKind::TestBlock(test) => {
            for stmt in &mut test.body {
                match stmt {
                    TestStatement::Assert(expr, _) | TestStatement::Expr(expr) => {
                        desugar_expr(expr);
                    }
                }
            }
        }
        ItemKind::Import(_) | ItemKind::TypeDecl(_) | ItemKind::TraitDecl(_) => {}
    }
}

fn desugar_function(decl: &mut FunctionDecl) {
    desugar_expr(&mut decl.body);
    for param in &mut decl.params {
        if let Some(default) = &mut param.default {
            desugar_expr(default);
        }
    }
}

fn desugar_expr(expr: &mut Expr) {
    desugar_children(expr);

    let span = expr.span;
    match &mut expr.kind {
        // Some(x) → x (Option is T | undefined at runtime)
        ExprKind::Some(inner) => {
            let inner = std::mem::replace(inner.as_mut(), Expr::synthetic(ExprKind::Unit, span));
            expr.kind = inner.kind;
            expr.span = inner.span;
        }
        // None → undefined
        ExprKind::None => {
            expr.kind = ExprKind::Identifier("undefined".to_string());
        }
        // Ok/Err are NOT desugared here because codegen emits `as const`
        // annotations that are needed for TypeScript discriminated union
        // narrowing. Moving Ok/Err desugaring here requires an AsConst
        // wrapper in the AST, which is a follow-up task.
        _ => {}
    }
}

/// Recurse into all child expressions of the given node.
fn desugar_children(expr: &mut Expr) {
    match &mut expr.kind {
        ExprKind::Binary { left, right, .. } | ExprKind::Pipe { left, right } => {
            desugar_expr(left);
            desugar_expr(right);
        }
        ExprKind::Unary { operand, .. } => desugar_expr(operand),
        ExprKind::Call { callee, args, .. } => {
            desugar_expr(callee);
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => desugar_expr(e),
                }
            }
        }
        ExprKind::Construct { args, spread, .. } => {
            if let Some(spread) = spread {
                desugar_expr(spread);
            }
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => desugar_expr(e),
                }
            }
        }
        ExprKind::Member { object, .. } => desugar_expr(object),
        ExprKind::Index { object, index } => {
            desugar_expr(object);
            desugar_expr(index);
        }
        ExprKind::Arrow { body, .. } => desugar_expr(body),
        ExprKind::Match { subject, arms } => {
            desugar_expr(subject);
            for arm in arms {
                desugar_expr(&mut arm.body);
                if let Some(guard) = &mut arm.guard {
                    desugar_expr(guard);
                }
            }
        }
        ExprKind::Await(inner)
        | ExprKind::Try(inner)
        | ExprKind::Unwrap(inner)
        | ExprKind::Ok(inner)
        | ExprKind::Err(inner)
        | ExprKind::Some(inner)
        | ExprKind::Grouped(inner)
        | ExprKind::Spread(inner) => {
            desugar_expr(inner);
        }
        ExprKind::Parse { value, .. } => desugar_expr(value),
        ExprKind::Block(items) | ExprKind::Collect(items) => {
            for item in items {
                desugar_item(item);
            }
        }
        ExprKind::Jsx(element) => desugar_jsx(element),
        ExprKind::Array(elems) | ExprKind::Tuple(elems) => {
            for e in elems {
                desugar_expr(e);
            }
        }
        ExprKind::Object(fields) => {
            for (_, e) in fields {
                desugar_expr(e);
            }
        }
        ExprKind::TemplateLiteral(parts) => {
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    desugar_expr(e);
                }
            }
        }
        ExprKind::DotShorthand { predicate, .. } => {
            if let Some((_, rhs)) = predicate {
                desugar_expr(rhs);
            }
        }
        // Leaf nodes — nothing to recurse into
        ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Bool(_)
        | ExprKind::Identifier(_)
        | ExprKind::Placeholder
        | ExprKind::None
        | ExprKind::Todo
        | ExprKind::Unreachable
        | ExprKind::Unit => {}
    }
}

fn desugar_jsx(element: &mut JsxElement) {
    if let JsxElementKind::Element { props, .. } = &mut element.kind {
        for prop in props {
            if let Some(value) = &mut prop.value {
                desugar_expr(value);
            }
        }
    }
    let children = match &mut element.kind {
        JsxElementKind::Element { children, .. } | JsxElementKind::Fragment { children } => {
            children
        }
    };
    for child in children {
        match child {
            JsxChild::Element(el) => desugar_jsx(el),
            JsxChild::Expr(e) => desugar_expr(e),
            JsxChild::Text(_) => {}
        }
    }
}
