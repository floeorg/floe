//! Desugar pass: transforms high-level AST constructs into simpler equivalents.
//!
//! Runs after the checker and before codegen. Each transform replaces a
//! language-level construct with lower-level AST nodes that codegen can
//! emit without needing semantic knowledge.
//!
//! Current transforms:
//! - `Some(x)` → `x` (identity — Option is `T | undefined`)
//! - `None`   → `Identifier("undefined")`
//! - Record constructors with omitted default fields → args filled in

use std::collections::{HashMap, HashSet};

use crate::parser::ast::*;
use crate::resolve::ResolvedImports;
use crate::walk;

/// Run the desugar pass over a program, transforming it in place.
pub fn desugar_program(program: &mut Program, resolved: &HashMap<String, ResolvedImports>) {
    // Collect type definitions for default field expansion
    let mut type_defs: HashMap<String, TypeDef> = HashMap::new();
    // Local types
    for item in &program.items {
        if let ItemKind::TypeDecl(decl) = &item.kind {
            type_defs.insert(decl.name.clone(), decl.def.clone());
        }
    }
    // Imported types
    for imports in resolved.values() {
        for decl in &imports.type_decls {
            type_defs.insert(decl.name.clone(), decl.def.clone());
        }
    }

    walk::walk_program_mut(program, &mut |expr| {
        desugar_expr(expr);
        expand_construct_defaults(expr, &type_defs);
    });
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

/// For record constructors with omitted fields that have defaults,
/// splice the default expressions into the arg list so codegen emits them.
/// Skipped when a spread is present — the spread provides all fields.
fn expand_construct_defaults(expr: &mut Expr, type_defs: &HashMap<String, TypeDef>) {
    let ExprKind::Construct {
        type_name,
        spread,
        args,
    } = &mut expr.kind
    else {
        return;
    };

    if spread.is_some() {
        return;
    }

    let Some(type_def) = type_defs.get(type_name.as_str()) else {
        return;
    };

    let provided: HashSet<String> = args
        .iter()
        .filter_map(|a| match a {
            Arg::Named { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect();

    let defaults: Vec<Arg> = type_def
        .record_fields()
        .iter()
        .filter(|f| !provided.contains(&f.name) && f.default.is_some())
        .map(|f| Arg::Named {
            label: f.name.clone(),
            value: f.default.clone().unwrap(),
        })
        .collect();

    args.extend(defaults);
}
