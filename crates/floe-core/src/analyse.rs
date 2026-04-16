//! Post-parse, pre-codegen orchestration.
//!
//! `analyse_module` is the single boundary between "we have source text" and
//! "we have a fully typed AST with diagnostics and reference data". Every
//! consumer — CLI `build`/`check`, LSP, tests, wasm playground — goes through
//! this function instead of wiring parse / resolve / type-check / lower by hand.
//!
//! The CLI owns the I/O (reading files, running tsgo, loading ambient types)
//! and hands the result in as `ModuleInputs`; `analyse_module` stays pure so
//! LSP can call it on every keystroke without touching disk.

use std::collections::{HashMap, HashSet};

use crate::checker::{self, Checker, lower_to_typed};
use crate::diagnostic::{self, Diagnostic};
use crate::interop::DtsExport;
use crate::interop::ambient::AmbientDeclarations;
use crate::parser::Parser;
use crate::parser::ast::TypedProgram;
use crate::reference::ReferenceTracker;
use crate::resolve::ResolvedImports;

/// Everything `analyse_module` needs beyond the raw source: resolved
/// imports (for cross-module type lookup), .d.ts exports, ambient types,
/// and the set of TypeScript imports that couldn't be resolved because
/// tsgo isn't installed.
#[derive(Debug, Default, Clone)]
pub struct ModuleInputs {
    pub resolved_imports: HashMap<String, ResolvedImports>,
    pub dts_imports: HashMap<String, Vec<DtsExport>>,
    pub ambient: Option<AmbientDeclarations>,
    pub ts_imports_missing_tsgo: HashSet<String>,
}

/// The result of analysing one module: a fully-typed program plus every
/// side-table downstream consumers care about.
pub struct AnalysedModule {
    pub program: TypedProgram,
    pub diagnostics: Vec<Diagnostic>,
    pub references: ReferenceTracker,
    pub resolved_imports: HashMap<String, ResolvedImports>,
}

/// Parse, type-check, and lower a single source file into an
/// `AnalysedModule`. Parse errors are returned as diagnostics rather than
/// an `Err` — they're still diagnostics the caller may want to render.
pub fn analyse_module(source: &str, inputs: ModuleInputs) -> AnalysedModule {
    match Parser::new(source).parse_program() {
        Ok(program) => analyse_parsed(program, inputs),
        Err(parse_errors) => AnalysedModule {
            program: TypedProgram {
                items: Vec::new(),
                span: crate::lexer::span::Span::new(0, 0, 1, 1),
            },
            diagnostics: diagnostic::from_parse_errors(&parse_errors),
            references: ReferenceTracker::new(),
            resolved_imports: inputs.resolved_imports,
        },
    }
}

/// Same as `analyse_module` but starts from a parsed `Program`. Useful
/// for tests that want to hand-build an AST, or callers that already
/// parsed for another reason.
pub fn analyse_parsed(
    program: crate::parser::ast::Program,
    inputs: ModuleInputs,
) -> AnalysedModule {
    let checker = Checker::from_context(
        inputs.resolved_imports.clone(),
        inputs.dts_imports,
        inputs.ambient,
        inputs.ts_imports_missing_tsgo,
    );
    let (diagnostics, expr_types, invalid_exprs, references) = check_and_collect(checker, &program);
    let typed = lower_to_typed(
        program,
        &expr_types,
        &invalid_exprs,
        &inputs.resolved_imports,
    );
    AnalysedModule {
        program: typed,
        diagnostics,
        references,
        resolved_imports: inputs.resolved_imports,
    }
}

/// Thin helper that runs the full checker pipeline and returns the four
/// outputs `analyse_parsed` needs. Wraps `Checker::check_all`'s tuple shape
/// so we can evolve the internal representation without changing callers.
fn check_and_collect(
    checker: Checker,
    program: &crate::parser::ast::Program,
) -> (
    Vec<Diagnostic>,
    checker::ExprTypeMap,
    HashSet<crate::parser::ast::ExprId>,
    ReferenceTracker,
) {
    let (diags, refs) = checker.check_full_with_references(program);
    let (diagnostics, expr_types, invalid_exprs) = diags;
    (diagnostics, expr_types, invalid_exprs, refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyses_a_trivial_module_without_errors() {
        let m = analyse_module("const _x = 42", ModuleInputs::default());
        let errors: Vec<_> = m
            .diagnostics
            .iter()
            .filter(|d| d.severity == diagnostic::Severity::Error)
            .collect();
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
        assert_eq!(m.program.items.len(), 1);
    }

    #[test]
    fn analyses_surface_parse_errors_as_diagnostics() {
        let m = analyse_module("const =", ModuleInputs::default());
        assert!(!m.diagnostics.is_empty(), "parse error should surface");
        assert!(m.program.items.is_empty());
    }

    #[test]
    fn analyses_populate_reference_tracker() {
        let m = analyse_module(
            r#"
fn greet(n: string) -> string { n }
const _a = greet("a")
"#,
            ModuleInputs::default(),
        );
        let def = m
            .references
            .definition_for_name("greet")
            .expect("greet definition registered");
        assert_eq!(m.references.find_references(def).len(), 1);
    }
}
