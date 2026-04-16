//! Post-parse, pre-codegen orchestration.
//!
//! `analyse_module` / `analyse_parsed` is the single boundary between
//! "we have source text" and "we have a fully typed AST with diagnostics
//! and reference data". The CLI owns I/O (reading files, running tsgo,
//! loading ambient types) and hands the result in as `ModuleInputs`;
//! analyse stays pure so the LSP can call it without touching disk.

use std::collections::{HashMap, HashSet};

use crate::checker::{Checker, lower_to_typed};
use crate::diagnostic::{self, Diagnostic};
use crate::interop::DtsExport;
use crate::interop::ambient::AmbientDeclarations;
use crate::parser::Parser;
use crate::parser::ast::TypedProgram;
use crate::reference::ReferenceTracker;
use crate::resolve::ResolvedImports;

/// Types pulled from outside Floe — TypeScript `.d.ts` exports, ambient
/// lib declarations, and the set of TS imports tsgo couldn't resolve.
/// Grouped because they all originate from the tsgo / ambient extern
/// pass and travel together through analyse.
#[derive(Debug, Default, Clone)]
pub struct ExternTypes {
    pub dts_imports: HashMap<String, Vec<DtsExport>>,
    pub ambient: Option<AmbientDeclarations>,
    pub ts_imports_missing_tsgo: HashSet<String>,
}

/// Everything `analyse_module` needs beyond raw source: resolved `.fl`
/// imports and the extern-type bundle.
#[derive(Debug, Default, Clone)]
pub struct ModuleInputs {
    pub resolved_imports: HashMap<String, ResolvedImports>,
    pub externs: ExternTypes,
}

/// A fully-typed program plus every side-table downstream consumers care about.
pub struct AnalysedModule {
    pub program: TypedProgram,
    pub diagnostics: Vec<Diagnostic>,
    pub references: ReferenceTracker,
    pub resolved_imports: HashMap<String, ResolvedImports>,
    /// `name → inferred type` display map. LSP completions and record
    /// field-accessor hovers key off this.
    pub name_types: HashMap<String, String>,
}

/// Parse, type-check, and lower a single source file. Parse errors come
/// back as diagnostics rather than an `Err` so callers can render
/// everything through the same path.
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
            name_types: HashMap::new(),
        },
    }
}

/// Start from a parsed `Program`. Callers that need the `Program` for
/// another reason (e.g. feeding it into `resolve_imports` before calling
/// analyse) use this to avoid re-parsing.
pub fn analyse_parsed(
    program: crate::parser::ast::Program,
    inputs: ModuleInputs,
) -> AnalysedModule {
    let checker = Checker::from_context(
        inputs.resolved_imports.clone(),
        inputs.externs.dts_imports,
        inputs.externs.ambient,
        inputs.externs.ts_imports_missing_tsgo,
    );
    let (diagnostics, name_types, expr_types, invalid_exprs, references) =
        checker.check_full_with_references(&program);
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
        name_types,
    }
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
