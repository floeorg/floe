//! Desugar pass: transforms high-level AST constructs into simpler equivalents.
//!
//! Runs after the checker and before codegen. Each transform replaces a
//! language-level construct with lower-level AST nodes that codegen can
//! emit without needing semantic knowledge.
//!
//! Current transforms:
//! - `Some(x)` (Construct) → `x` (identity — Option is `T | undefined`)
//! - `None` (Construct or Identifier) → `Identifier("undefined")`
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

    // Function signatures keyed by name. Nested functions with the same
    // name as an outer binding lose the insertion race, but nested
    // shadowing of a top-level name is rare and doesn't reproduce the
    // reorder bug in practice.
    let mut fn_signatures: HashMap<String, Vec<Param>> = HashMap::new();
    for item in &program.items {
        if let ItemKind::Function(decl) = &item.kind {
            fn_signatures.insert(decl.name.clone(), decl.params.clone());
        }
    }
    for imports in resolved.values() {
        for decl in &imports.function_decls {
            fn_signatures
                .entry(decl.name.clone())
                .or_insert_with(|| decl.params.clone());
        }
    }

    walk::walk_program_mut(program, &mut |expr| {
        desugar_expr(expr);
        expand_construct_defaults(expr, &type_defs);
        reorder_call_named_args(expr, &fn_signatures);
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
        ExprKind::Construct {
            type_name,
            args,
            spread: None,
        } if type_name == crate::type_layout::VARIANT_SOME && args.len() == 1 => {
            if let Some(Arg::Positional(inner)) = args.pop() {
                expr.kind = inner.kind;
                expr.span = inner.span;
            }
        }
        // None → undefined (when used as a construct with no args)
        ExprKind::Construct {
            type_name,
            args,
            spread: None,
        } if type_name == crate::type_layout::VARIANT_NONE && args.is_empty() => {
            expr.kind = ExprKind::Identifier("undefined".to_string());
        }
        // None → undefined (when used as a bare identifier)
        ExprKind::Identifier(name) if name == crate::type_layout::VARIANT_NONE => {
            expr.kind = ExprKind::Identifier("undefined".to_string());
        }
        // Value(x) → x (Settable wraps value directly)
        ExprKind::Value(inner) => {
            let inner = std::mem::replace(inner.as_mut(), Expr::synthetic(ExprKind::Unit, span));
            expr.kind = inner.kind;
            expr.span = inner.span;
        }
        // Clear → null
        ExprKind::Clear => {
            expr.kind = ExprKind::Identifier("null".to_string());
        }
        // Unchanged is NOT desugared — codegen detects it and omits the field
        // Ok/Err are now regular Construct expressions — codegen handles them
        // in the Construct branch (emitting `as const` for TS discriminated unions).
        _ => {}
    }
}

/// Reorder a `Call`'s named arguments into declared-parameter order and
/// splice defaults for omitted slots so codegen can keep its label-
/// erasing emit. Sibling of `expand_construct_defaults`.
fn reorder_call_named_args(expr: &mut Expr, fn_signatures: &HashMap<String, Vec<Param>>) {
    let ExprKind::Call { callee, args, .. } = &mut expr.kind else {
        return;
    };
    let ExprKind::Identifier(name) = &callee.kind else {
        return;
    };
    let Some(params) = fn_signatures.get(name) else {
        return;
    };

    let has_named = args.iter().any(|a| matches!(a, Arg::Named { .. }));
    if !has_named {
        return;
    }

    let original = std::mem::take(args);
    let mut positional: Vec<Arg> = Vec::new();
    let mut named: Vec<(String, Arg)> = Vec::new();
    for arg in original {
        match arg {
            Arg::Positional(_) if named.is_empty() => positional.push(arg),
            Arg::Named { ref label, .. } => named.push((label.clone(), arg)),
            // Positional after named is a checker error; drop the arg
            // since any codegen output for this call is already invalid.
            Arg::Positional(_) => {}
        }
    }

    let mut reordered = positional;
    for param in params.iter().skip(reordered.len()) {
        if let Some(pos) = named.iter().position(|(l, _)| l == &param.name) {
            reordered.push(named.remove(pos).1);
        } else if let Some(default) = &param.default {
            reordered.push(Arg::Named {
                label: param.name.clone(),
                value: default.clone(),
            });
        }
    }
    // Unknown labels / duplicates stay in source order so the checker's
    // diagnostics anchor to their original spans.
    reordered.extend(named.into_iter().map(|(_, a)| a));

    *args = reordered;
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
