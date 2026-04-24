use serde::Serialize;
use wasm_bindgen::prelude::*;

use floe_core::checker::Checker;
use floe_core::codegen::Codegen;
use floe_core::diagnostic;
use floe_core::parser::Parser;

/// A diagnostic message returned to JavaScript.
#[derive(Serialize)]
pub struct JsDiagnostic {
    pub severity: String,
    pub message: String,
    pub line: u32,
    pub column: u32,
    pub code: Option<String>,
}

/// The result of compiling Floe source code.
#[derive(Serialize)]
pub struct CompileResult {
    pub output: String,
    pub diagnostics: Vec<JsDiagnostic>,
    pub has_jsx: bool,
    pub success: bool,
}

/// Compile Floe source to TypeScript.
///
/// Returns a JSON-serialized `CompileResult` with the output code
/// and any diagnostics (errors/warnings).
#[wasm_bindgen]
pub fn compile(source: &str) -> JsValue {
    let result = compile_inner(source);
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

fn compile_inner(source: &str) -> CompileResult {
    let mut all_diagnostics = Vec::new();

    // Parse
    let program = match Parser::new(source).parse_program() {
        Ok(program) => program,
        Err(errors) => {
            let diags = diagnostic::from_parse_errors(&errors);
            for d in &diags {
                all_diagnostics.push(JsDiagnostic {
                    severity: format!("{:?}", d.severity),
                    message: d.message.clone(),
                    line: d.span.line as u32,
                    column: d.span.column as u32,
                    code: d.code.clone(),
                });
            }
            return CompileResult {
                output: String::new(),
                diagnostics: all_diagnostics,
                has_jsx: false,
                success: false,
            };
        }
    };

    // Type check
    let (check_diags, expr_types, invalid_exprs, shadowed_keywords) =
        Checker::new().check_full(&program);
    let has_errors = check_diags
        .iter()
        .any(|d| d.severity == diagnostic::Severity::Error);

    for d in &check_diags {
        all_diagnostics.push(JsDiagnostic {
            severity: format!("{:?}", d.severity),
            message: d.message.clone(),
            line: d.span.line as u32,
            column: d.span.column as u32,
            code: d.code.clone(),
        });
    }

    // Convert to typed AST for codegen (even with type errors, for playground preview).
    let typed_program =
        floe_core::checker::attach_types(program, &expr_types, &invalid_exprs, &shadowed_keywords);
    let output = Codegen::new().generate(&typed_program);

    CompileResult {
        output: output.code,
        diagnostics: all_diagnostics,
        has_jsx: output.has_jsx,
        success: !has_errors,
    }
}

/// Check Floe source without generating output.
/// Returns diagnostics only.
#[wasm_bindgen]
pub fn check(source: &str) -> JsValue {
    let mut all_diagnostics = Vec::new();

    match Parser::new(source).parse_program() {
        Ok(program) => {
            let check_diags = Checker::new().check(&program);
            for d in &check_diags {
                all_diagnostics.push(JsDiagnostic {
                    severity: format!("{:?}", d.severity),
                    message: d.message.clone(),
                    line: d.span.line as u32,
                    column: d.span.column as u32,
                    code: d.code.clone(),
                });
            }
        }
        Err(errors) => {
            let diags = diagnostic::from_parse_errors(&errors);
            for d in &diags {
                all_diagnostics.push(JsDiagnostic {
                    severity: format!("{:?}", d.severity),
                    message: d.message.clone(),
                    line: d.span.line as u32,
                    column: d.span.column as u32,
                    code: d.code.clone(),
                });
            }
        }
    }

    serde_wasm_bindgen::to_value(&all_diagnostics).unwrap_or(JsValue::NULL)
}

/// Get the compiler version.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
