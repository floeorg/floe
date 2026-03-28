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
use crate::walk;

/// Run the desugar pass over a program, transforming it in place.
pub fn desugar_program(program: &mut Program) {
    walk::walk_program_mut(program, &mut desugar_expr);
}

/// Desugar is post-order: we need children desugared before transforming
/// the current node. `walk_program_mut` calls us in pre-order, but we
/// only transform leaf-like patterns (Some/None) that don't depend on
/// child desugaring order, so pre-order is safe here.
fn desugar_expr(expr: &mut Expr) {
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
        // annotations needed for TypeScript discriminated union narrowing.
        _ => {}
    }
}
