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
//!
//! A transform that splices a copy of a declared default stamps
//! `ExprId::SYNTHETIC` on every node of the copy. The id keys the
//! checker's three maps, and the declaration can come from another file,
//! where the same number names a different expression (#1533).

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{Arg, Expr, ExprId, ExprKind, ItemKind, Param, Program, TypeDef};
use crate::resolve::ResolvedImports;
use crate::walk;

/// Run the desugar pass over a program, transforming it in place.
pub fn desugar_program(program: &mut Program, resolved: &HashMap<String, ResolvedImports>) {
    // Gather per-module metadata the transforms need. Nested functions
    // that shadow a top-level name lose the insertion race, but shadowing
    // top-level names is rare enough not to matter in practice.
    let mut type_defs: HashMap<String, TypeDef> = HashMap::new();
    let mut fn_signatures: HashMap<String, Vec<Param>> = HashMap::new();
    for item in &program.items {
        match &item.kind {
            ItemKind::TypeDecl(decl) => {
                type_defs.insert(decl.name.clone(), decl.def.clone());
            }
            ItemKind::Function(decl) => {
                fn_signatures.insert(decl.name.clone(), decl.params.clone());
            }
            _ => {}
        }
    }
    for imports in resolved.values() {
        for decl in &imports.type_decls {
            type_defs
                .entry(decl.name.clone())
                .or_insert_with(|| decl.def.clone());
        }
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
            ..
        } if type_name == crate::type_layout::VARIANT_SOME && args.len() == 1 => {
            if let Some(Arg::Positional(inner)) = args.pop() {
                // Take the child's id with its kind. The id keys the type
                // map, so a node that carries the child's kind under the
                // parent's id reads back the `Option<T>` the parent had
                // instead of the `T` the emitted expression really is.
                expr.id = inner.id;
                expr.kind = inner.kind;
                expr.span = inner.span;
            }
        }
        // None → undefined (when used as a construct with no args)
        ExprKind::Construct {
            type_name,
            args,
            spread: None,
            ..
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
            expr.id = inner.id;
            expr.kind = inner.kind;
            expr.span = inner.span;
        }
        // Clear → null
        ExprKind::Clear => {
            expr.kind = ExprKind::Identifier("null".to_string());
        }
        // `None` and `Clear` keep their ids on purpose. The id still names
        // the same source expression, and the type the checker gave it is
        // still the right one for the node. `Clear` also needs it:
        // `attach_types` reads `shadowed_keyword_exprs` by id and rewrites
        // a shadowed `Clear` back into the local binding it names, which a
        // synthetic id would throw away.
        // Unchanged is NOT desugared — codegen detects it and omits the field
        // Ok/Err are now regular Construct expressions — codegen handles them
        // in the Construct branch (emitting `as const` for TS discriminated unions).
        _ => {}
    }
}

/// Stamp `ExprId::SYNTHETIC` on every node of a default expression that
/// a transform splices into the tree.
///
/// A default lives in a declaration, and the declaration can come from
/// another file (`resolve.rs` hands desugar the imported `TypeDecl`s and
/// `FunctionDecl`s). Its nodes carry the ids that file's `ExprIdGen`
/// handed out, and those ids mean something else in this file:
/// `attach_types` reads `ExprTypeMap`, `invalid_exprs` and
/// `shadowed_keyword_exprs` by id, so a foreign default takes this
/// file's answer for the same number. A default declared in this file
/// duplicates an id instead, which is benign but breaks the "one id, one
/// expression" rule the type map depends on.
///
/// `SYNTHETIC` is absent from all three maps, so a stamped node reads
/// back the `UNKNOWN` sentinel and keeps its own kind. It costs the
/// spliced copy its type; see the module tests.
fn stamp_synthetic(expr: &mut Expr) {
    walk::walk_expr_mut(expr, &mut |e| e.id = ExprId::SYNTHETIC);
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
            let mut value = default.clone();
            stamp_synthetic(&mut value);
            reordered.push(Arg::Named {
                label: param.name.clone(),
                value,
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
        ..
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
            Arg::Positional(_) => None,
        })
        .collect();

    let defaults: Vec<Arg> = type_def
        .record_fields()
        .iter()
        .filter(|f| !provided.contains(&f.name) && f.default.is_some())
        .map(|f| {
            let mut value = f.default.clone().unwrap();
            stamp_synthetic(&mut value);

            Arg::Named {
                label: f.name.clone(),
                value,
            }
        })
        .collect();

    args.extend(defaults);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::{Checker, lower_to_typed};
    use crate::codegen::Codegen;
    use crate::parser::Parser;
    use crate::parser::ast::ExprId;

    fn parse(source: &str) -> Program {
        Parser::new(source).parse_program().unwrap_or_else(|errs| {
            panic!(
                "parse failed:\n{}",
                errs.iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
    }

    /// Build what `resolve.rs` hands desugar for `import ... from "./dep"`.
    /// Every declaration keeps the ids `dep`'s own `ExprIdGen` gave it,
    /// and those ids start at 0 again, the same as this file's.
    fn imports_from(dep: &str) -> HashMap<String, ResolvedImports> {
        let mut imports = ResolvedImports::default();
        for item in parse(dep).items {
            match item.kind {
                ItemKind::TypeDecl(decl) => imports.type_decls.push(decl),
                ItemKind::Function(decl) => imports.function_decls.push(decl),
                _ => {}
            }
        }

        HashMap::from([("./dep".to_string(), imports)])
    }

    /// Run the production pipeline over `main`: check, desugar, attach
    /// types, emit TypeScript. `lower_to_typed` is the same call the CLI
    /// and the LSP make.
    fn emit_with_dep(main: &str, dep: &str) -> String {
        let resolved = imports_from(dep);
        let program = parse(main);
        let (_diags, expr_types, invalid, shadowed) =
            Checker::with_imports(resolved.clone()).check_full(&program);
        let typed = lower_to_typed(program, &expr_types, &invalid, &shadowed, &resolved);

        Codegen::with_imports(&resolved)
            .generate(&typed)
            .code
            .trim()
            .to_string()
    }

    /// Desugar over one file, with no imports.
    fn desugar(source: &str) -> Program {
        let mut program = parse(source);
        desugar_program(&mut program, &HashMap::new());

        program
    }

    // ── #1533: a spliced default carries no foreign id ──────────────

    #[test]
    fn a_foreign_record_default_keeps_its_own_value_when_an_id_collides() {
        // `222` is `ExprId(1)` in dep. `Unchanged` is `ExprId(1)` in main
        // and sits in `shadowed_keyword_exprs`, so before #1533
        // `attach_types` rewrote the spliced literal into `unchanged`.
        let out = emit_with_dep(
            r#"
import { Cfg } from "./dep"
export let f() -> number = { let unchanged = 7  Unchanged }
export let g() -> Cfg = { Cfg { a: 9 } }
"#,
            "export type Cfg = { a: number = 111, b: number = 222 }",
        );
        assert!(
            out.contains("return { a: 9, b: 222 };"),
            "the spliced default must stay `222`, got:\n{out}"
        );
    }

    #[test]
    fn a_foreign_parameter_default_keeps_its_own_value_when_an_id_collides() {
        // Same collision through `reorder_call_named_args`: `222` is
        // `ExprId(3)` in dep, and main's `Unchanged` is `ExprId(3)`.
        let out = emit_with_dep(
            r#"
import { h } from "./dep"
export let f() -> number = { let unchanged = 7  let p1 = 1  let p2 = 2  Unchanged }
export let g() -> number = { h(a: 9) }
"#,
            r#"
export let pad(x: number = 1) -> number = { x }
export let h(a: number, b: number = 222) -> number = { a + b }
"#,
        );
        assert!(
            out.contains("return h(9, 222);"),
            "the spliced default must stay `222`, got:\n{out}"
        );
    }

    #[test]
    fn a_foreign_default_that_is_itself_a_construct_splices_whole() {
        // A nested splice: `Outer`'s default for `inner` is `Inner { }`,
        // which desugar then expands with `Inner`'s own default. Both
        // levels are foreign. `Inner { }` is `ExprId(1)` in dep, and
        // main's shadowed `Unchanged` is `ExprId(1)`, so before #1533
        // `attach_types` rewrote the whole spliced record into a name.
        let out = emit_with_dep(
            r#"
import { Outer } from "./dep"
export let f() -> number = { let unchanged = 7  Unchanged }
export let g() -> Outer = { Outer { tag: "y" } }
"#,
            r#"
export type Inner = { n: number = 1 }
export type Outer = { inner: Inner = Inner { }, tag: string = "x" }
"#,
        );
        assert!(
            out.contains(r#"return { tag: "y", inner: { n: 1 } };"#),
            "the nested default must stay `{{ n: 1 }}`, got:\n{out}"
        );
    }

    #[test]
    fn a_spliced_foreign_default_keeps_the_name_it_was_written_with() {
        // The splice still reaches a name that main cannot see. #1533
        // stops the id corruption; it does not give `base` a binding
        // here, and no pass reports that. Tracked separately.
        let out = emit_with_dep(
            r#"
import { Cfg } from "./dep"
export let g() -> Cfg = { Cfg { b: 9 } }
"#,
            r#"
let base = 5
export type Cfg = { a: number = base, b: number = 2 }
"#,
        );
        assert!(
            out.contains("return { b: 9, a: base };"),
            "expected the default to splice as written, got:\n{out}"
        );
    }

    #[test]
    fn every_node_of_a_spliced_default_is_synthetic() {
        let mut program = parse(
            r#"
import { Cfg } from "./dep"
export let g() -> Cfg = { Cfg { a: 9 } }
"#,
        );
        let resolved = imports_from("export type Cfg = { a: number = 1, b: number = 2 + 3 }");
        desugar_program(&mut program, &resolved);

        let mut spliced = Vec::new();
        walk::walk_program_mut(&mut program, &mut |expr| {
            if let ExprKind::Construct { args, .. } = &expr.kind {
                for arg in args {
                    if let Arg::Named { label, value } = arg
                        && label == "b"
                    {
                        walk::walk_expr(value, &mut |e: &Expr| spliced.push(e.id));
                    }
                }
            }
        });
        assert_eq!(spliced.len(), 3, "expected `2 + 3` and its two operands");
        assert!(
            spliced.iter().all(|id| *id == ExprId::SYNTHETIC),
            "every spliced node needs the synthetic id, got {spliced:?}"
        );
    }

    #[test]
    fn some_hands_its_id_to_the_expression_it_unwraps() {
        let program = desugar("export let f(x: number) -> Option<number> = { Some(x) }");
        let mut ids = Vec::new();
        crate::walk::walk_program(&program, &mut |e: &Expr| {
            if matches!(&e.kind, ExprKind::Identifier(n) if n == "x") {
                ids.push(e.id);
            }
        });
        assert_eq!(ids.len(), 1, "expected one `x`, got {ids:?}");
        assert_ne!(
            ids[0],
            ExprId::SYNTHETIC,
            "the unwrapped node keeps the id the checker typed"
        );
    }

    #[test]
    fn a_shadowed_clear_keyword_survives_desugar() {
        // Desugar rewrites `Clear` to `null` without knowing about
        // shadowing, and `attach_types` puts the binding back by id.
        // A synthetic id here would emit `null` for a name the user
        // wrote, so `Clear` keeps its id.
        let out = emit_with_dep(
            "export let f() -> number = { let clear = 7  Clear }",
            "export type Unused = { a: number = 1 }",
        );
        assert!(
            out.contains("return clear;"),
            "expected the shadowed binding, got:\n{out}"
        );
    }
}
