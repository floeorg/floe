//! Shared AST walker for mutable expression traversal.
//!
//! Provides `walk_expr_mut` and `walk_item_mut` that handle the structural
//! recursion into all ExprKind/ItemKind variants. Callers supply a callback
//! for the action to perform at each expression node.
//!
//! Every function is generic over the AST phase type `T` so the same
//! walker services both `UntypedExpr = Expr<()>` (pre-check) and
//! `TypedExpr = Expr<Arc<Type>>` (post-check).

use crate::parser::ast::*;

/// Walk all expressions in a program, calling `f` on each one.
/// The callback receives each `&mut Expr` in pre-order (parent before children).
pub fn walk_program_mut<T>(program: &mut Program<T>, f: &mut impl FnMut(&mut Expr<T>)) {
    for item in &mut program.items {
        walk_item_mut(item, f);
    }
}

pub fn walk_item_mut<T>(item: &mut Item<T>, f: &mut impl FnMut(&mut Expr<T>)) {
    match &mut item.kind {
        ItemKind::Expr(expr) => walk_expr_mut(expr, f),
        ItemKind::Const(decl) => walk_expr_mut(&mut decl.value, f),
        ItemKind::Function(decl) => walk_function_mut(decl, f),
        ItemKind::ForBlock(block) => {
            for func in &mut block.functions {
                walk_function_mut(func, f);
            }
        }
        ItemKind::TestBlock(test) => {
            for stmt in &mut test.body {
                match stmt {
                    TestStatement::Assert(expr, _) | TestStatement::Expr(expr) => {
                        walk_expr_mut(expr, f);
                    }
                }
            }
        }
        ItemKind::Import(_)
        | ItemKind::ReExport(_)
        | ItemKind::TypeDecl(_)
        | ItemKind::TraitDecl(_) => {}
    }
}

pub fn walk_function_mut<T>(decl: &mut FunctionDecl<T>, f: &mut impl FnMut(&mut Expr<T>)) {
    walk_expr_mut(&mut decl.body, f);
    for param in &mut decl.params {
        if let Some(default) = &mut param.default {
            walk_expr_mut(default, f);
        }
    }
}

/// Walk an expression tree, calling `f` on each node in pre-order.
/// Recurses into all children after calling `f` on the current node.
pub fn walk_expr_mut<T>(expr: &mut Expr<T>, f: &mut impl FnMut(&mut Expr<T>)) {
    f(expr);
    walk_expr_children_mut(expr, f);
}

/// Walk only the children of an expression (not the expression itself).
/// Useful when the caller needs post-order traversal (children first, then self).
pub fn walk_expr_children_mut<T>(expr: &mut Expr<T>, f: &mut impl FnMut(&mut Expr<T>)) {
    match &mut expr.kind {
        ExprKind::Binary { left, right, .. } | ExprKind::Pipe { left, right } => {
            walk_expr_mut(left, f);
            walk_expr_mut(right, f);
        }
        ExprKind::Unary { operand, .. } => walk_expr_mut(operand, f),
        ExprKind::Call { callee, args, .. } => {
            walk_expr_mut(callee, f);
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => walk_expr_mut(e, f),
                }
            }
        }
        ExprKind::Construct { args, spread, .. } => {
            if let Some(s) = spread {
                walk_expr_mut(s, f);
            }
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => walk_expr_mut(e, f),
                }
            }
        }
        ExprKind::Member { object, .. } => walk_expr_mut(object, f),
        ExprKind::Index { object, index } => {
            walk_expr_mut(object, f);
            walk_expr_mut(index, f);
        }
        ExprKind::Arrow { body, .. } => walk_expr_mut(body, f),
        ExprKind::Match { subject, arms } => {
            walk_expr_mut(subject, f);
            for arm in arms {
                walk_expr_mut(&mut arm.body, f);
                if let Some(guard) = &mut arm.guard {
                    walk_expr_mut(guard, f);
                }
            }
        }
        ExprKind::Unwrap(inner)
        | ExprKind::Value(inner)
        | ExprKind::Grouped(inner)
        | ExprKind::Spread(inner) => walk_expr_mut(inner, f),
        ExprKind::Parse { value, .. } => walk_expr_mut(value, f),
        ExprKind::Mock { overrides, .. } => {
            for arg in overrides {
                match arg {
                    Arg::Positional(e) => walk_expr_mut(e, f),
                    Arg::Named { value, .. } => walk_expr_mut(value, f),
                }
            }
        }
        ExprKind::Block(items) | ExprKind::Collect(items) => {
            for item in items {
                walk_item_mut(item, f);
            }
        }
        ExprKind::Jsx(element) => walk_jsx_mut(element, f),
        ExprKind::Array(elems) | ExprKind::Tuple(elems) => {
            for e in elems {
                walk_expr_mut(e, f);
            }
        }
        ExprKind::Object(fields) => {
            for (_, e) in fields {
                walk_expr_mut(e, f);
            }
        }
        ExprKind::TemplateLiteral(parts) => {
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    walk_expr_mut(e, f);
                }
            }
        }
        ExprKind::TaggedTemplate { tag, parts } => {
            walk_expr_mut(tag, f);
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    walk_expr_mut(e, f);
                }
            }
        }
        ExprKind::DotShorthand { predicate, .. } => {
            if let Some((_, rhs)) = predicate {
                walk_expr_mut(rhs, f);
            }
        }
        // Leaf nodes
        ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Bool(_)
        | ExprKind::Identifier(_)
        | ExprKind::Placeholder
        | ExprKind::Clear
        | ExprKind::Unchanged
        | ExprKind::Todo
        | ExprKind::Unreachable
        | ExprKind::Unit
        | ExprKind::Invalid => {}
    }
}

// ── Immutable walker (for read-only traversal) ──────────────────

/// Walk an entire program immutably, calling f on every expression.
pub fn walk_program<T>(program: &Program<T>, f: &mut impl FnMut(&Expr<T>)) {
    for item in &program.items {
        walk_item(item, f);
    }
}

/// Walk an expression and all its children, calling f on each.
pub fn walk_expr<T>(expr: &Expr<T>, f: &mut impl FnMut(&Expr<T>)) {
    f(expr);
    walk_expr_children(expr, f);
}

/// Walk only the children of an expression (not the expression itself).
pub fn walk_expr_children<T>(expr: &Expr<T>, f: &mut impl FnMut(&Expr<T>)) {
    match &expr.kind {
        ExprKind::Binary { left, right, .. } | ExprKind::Pipe { left, right } => {
            walk_expr(left, f);
            walk_expr(right, f);
        }
        ExprKind::Unary { operand, .. } => {
            walk_expr(operand, f);
        }
        ExprKind::Call { callee, args, .. } => {
            walk_expr(callee, f);
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        walk_expr(e, f);
                    }
                }
            }
        }
        ExprKind::Construct { args, spread, .. } => {
            if let Some(s) = spread {
                walk_expr(s, f);
            }
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        walk_expr(e, f);
                    }
                }
            }
        }
        ExprKind::Member { object, .. } => {
            walk_expr(object, f);
        }
        ExprKind::Index { object, index } => {
            walk_expr(object, f);
            walk_expr(index, f);
        }
        ExprKind::Arrow { body, .. } => {
            walk_expr(body, f);
        }
        ExprKind::Match { subject, arms } => {
            walk_expr(subject, f);
            for arm in arms {
                walk_expr(&arm.body, f);
                if let Some(guard) = &arm.guard {
                    walk_expr(guard, f);
                }
            }
        }
        ExprKind::Unwrap(inner)
        | ExprKind::Value(inner)
        | ExprKind::Grouped(inner)
        | ExprKind::Spread(inner) => {
            walk_expr(inner, f);
        }
        ExprKind::Parse { value, .. } => {
            walk_expr(value, f);
        }
        ExprKind::Mock { overrides, .. } => {
            for arg in overrides {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        walk_expr(e, f);
                    }
                }
            }
        }
        ExprKind::Block(items) | ExprKind::Collect(items) => {
            for item in items {
                walk_item(item, f);
            }
        }
        ExprKind::Jsx(element) => walk_jsx_children(element, f),
        ExprKind::Array(elems) | ExprKind::Tuple(elems) => {
            for e in elems {
                walk_expr(e, f);
            }
        }
        ExprKind::Object(fields) => {
            for (_, e) in fields {
                walk_expr(e, f);
            }
        }
        ExprKind::TemplateLiteral(parts) => {
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    walk_expr(e, f);
                }
            }
        }
        ExprKind::TaggedTemplate { tag, parts } => {
            walk_expr(tag, f);
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    walk_expr(e, f);
                }
            }
        }
        ExprKind::DotShorthand { predicate, .. } => {
            if let Some((_, rhs)) = predicate {
                walk_expr(rhs, f);
            }
        }
        ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Bool(_)
        | ExprKind::Identifier(_)
        | ExprKind::Placeholder
        | ExprKind::Clear
        | ExprKind::Unchanged
        | ExprKind::Todo
        | ExprKind::Unreachable
        | ExprKind::Unit
        | ExprKind::Invalid => {}
    }
}

pub fn walk_item<T>(item: &Item<T>, f: &mut impl FnMut(&Expr<T>)) {
    match &item.kind {
        ItemKind::Const(decl) => {
            walk_expr(&decl.value, f);
        }
        ItemKind::Function(decl) => {
            walk_expr(&decl.body, f);
        }
        ItemKind::ForBlock(block) => {
            for func in &block.functions {
                walk_expr(&func.body, f);
            }
        }
        ItemKind::Expr(expr) => {
            walk_expr(expr, f);
        }
        _ => {}
    }
}

fn walk_jsx_children<T>(element: &JsxElement<T>, f: &mut impl FnMut(&Expr<T>)) {
    match &element.kind {
        JsxElementKind::Element {
            props, children, ..
        } => {
            for prop in props {
                match prop {
                    JsxProp::Named { value, .. } => {
                        if let Some(value) = value {
                            walk_expr(value, f);
                        }
                    }
                    JsxProp::Spread { expr, .. } => {
                        walk_expr(expr, f);
                    }
                }
            }
            for child in children {
                match child {
                    JsxChild::Expr(e) => {
                        walk_expr(e, f);
                    }
                    JsxChild::Element(el) => walk_jsx_children(el, f),
                    JsxChild::Text(_) => {}
                }
            }
        }
        JsxElementKind::Fragment { children } => {
            for child in children {
                match child {
                    JsxChild::Expr(e) => {
                        walk_expr(e, f);
                    }
                    JsxChild::Element(el) => walk_jsx_children(el, f),
                    JsxChild::Text(_) => {}
                }
            }
        }
    }
}

fn walk_jsx_mut<T>(element: &mut JsxElement<T>, f: &mut impl FnMut(&mut Expr<T>)) {
    if let JsxElementKind::Element { props, .. } = &mut element.kind {
        for prop in props {
            match prop {
                JsxProp::Named { value, .. } => {
                    if let Some(value) = value {
                        walk_expr_mut(value, f);
                    }
                }
                JsxProp::Spread { expr, .. } => {
                    walk_expr_mut(expr, f);
                }
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
            JsxChild::Element(el) => walk_jsx_mut(el, f),
            JsxChild::Expr(e) => walk_expr_mut(e, f),
            JsxChild::Text(_) => {}
        }
    }
}
