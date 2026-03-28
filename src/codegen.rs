mod expr;
mod jsx;
mod match_emit;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use crate::checker::ExprTypeMap;
use crate::parser::ast::*;
use crate::resolve::ResolvedImports;
use crate::stdlib::StdlibRegistry;
use crate::type_names;

const TAG_FIELD: &str = "tag";
const OK_FIELD: &str = "ok";
const VALUE_FIELD: &str = "value";
const ERROR_FIELD: &str = "error";

/// Code generation result: the emitted TypeScript source and whether it contains JSX.
pub struct CodegenOutput {
    pub code: String,
    pub has_jsx: bool,
    /// Declaration stub content for `.d.ts` files.
    pub dts: String,
}

/// A single step in a flattened pipe+unwrap chain.
struct PipeStep {
    /// The expression for this step.
    /// For the base (first) step, this is the original expression.
    /// For pipe steps, this is the "right" side of the pipe.
    expr: Expr,
    /// Whether this step has `?` (needs Result unwrap with early return).
    unwrap: bool,
    /// Whether this step is wrapped in `await`.
    is_await: bool,
    /// Whether this is a pipe step (true) or the base expression (false).
    is_pipe: bool,
}

/// The Floe code generator. Emits clean, readable TypeScript / TSX.
pub struct Codegen {
    output: String,
    indent: usize,
    has_jsx: bool,
    needs_deep_equal: bool,
    unwrap_counter: usize,
    stdlib: StdlibRegistry,
    /// Names that are zero-arg union variants (e.g. "All", "Empty")
    unit_variants: HashSet<String>,
    /// Maps variant name -> (union_type_name, field_names)
    variant_info: HashMap<String, (String, Vec<String>)>,
    /// Locally defined function/const names - these shadow stdlib in pipe resolution
    local_names: HashSet<String>,
    /// Expression type map from the checker, keyed by span (start, end).
    /// Used for type-directed pipe resolution.
    expr_types: ExprTypeMap,
    /// Resolved imports from other .fl files, for expanding bare imports.
    resolved_imports: HashMap<String, ResolvedImports>,
    /// Maps original import name -> aliased name for names that conflict with locals.
    import_aliases: HashMap<String, String>,
    /// Whether to emit test blocks (true for `floe test`, false for `floe build`).
    test_mode: bool,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            has_jsx: false,
            needs_deep_equal: false,
            unwrap_counter: 0,
            stdlib: StdlibRegistry::new(),
            unit_variants: HashSet::new(),
            variant_info: HashMap::new(),
            local_names: HashSet::new(),
            expr_types: HashMap::new(),
            resolved_imports: HashMap::new(),
            import_aliases: HashMap::new(),
            test_mode: false,
        }
    }

    /// Enable test mode: test blocks will be emitted instead of stripped.
    pub fn with_test_mode(mut self) -> Self {
        self.test_mode = true;
        self
    }

    /// Create a codegen with expression type information from the checker.
    pub fn with_expr_types(expr_types: ExprTypeMap) -> Self {
        Self {
            expr_types,
            ..Self::new()
        }
    }

    /// Create a codegen with expression types and resolved import info.
    pub fn with_imports(
        expr_types: ExprTypeMap,
        resolved: &HashMap<String, ResolvedImports>,
    ) -> Self {
        let mut codegen = Self::with_expr_types(expr_types);
        codegen.resolved_imports = resolved.clone();
        // Pre-register union variant info from imported types
        for imports in resolved.values() {
            for decl in &imports.type_decls {
                if let TypeDef::Union(variants) = &decl.def {
                    for variant in variants {
                        let field_names: Vec<String> = variant
                            .fields
                            .iter()
                            .enumerate()
                            .map(|(i, f)| {
                                f.name.clone().unwrap_or_else(|| {
                                    if variant.fields.len() == 1 {
                                        "value".to_string()
                                    } else {
                                        format!("_{i}")
                                    }
                                })
                            })
                            .collect();
                        if variant.fields.is_empty() {
                            codegen.unit_variants.insert(variant.name.clone());
                        }
                        codegen
                            .variant_info
                            .insert(variant.name.clone(), (decl.name.clone(), field_names));
                    }
                }
            }
        }
        codegen
    }

    /// Generate TypeScript from a Floe program.
    pub fn generate(mut self, program: &Program) -> CodegenOutput {
        // First pass: collect union variant info and local names
        for item in &program.items {
            match &item.kind {
                ItemKind::TypeDecl(decl) => {
                    if let TypeDef::Union(variants) = &decl.def {
                        for variant in variants {
                            let field_names: Vec<String> = variant
                                .fields
                                .iter()
                                .enumerate()
                                .map(|(i, f)| {
                                    f.name.clone().unwrap_or_else(|| {
                                        if variant.fields.len() == 1 {
                                            "value".to_string()
                                        } else {
                                            format!("_{i}")
                                        }
                                    })
                                })
                                .collect();
                            if variant.fields.is_empty() {
                                self.unit_variants.insert(variant.name.clone());
                            }
                            self.variant_info
                                .insert(variant.name.clone(), (decl.name.clone(), field_names));
                        }
                    }
                    // Register derived function names as local names
                    for trait_name in &decl.deriving {
                        if trait_name.as_str() == "Display" {
                            self.local_names.insert("display".to_string());
                        }
                    }
                }
                ItemKind::Function(decl) => {
                    self.local_names.insert(decl.name.clone());
                }
                ItemKind::Const(decl) => {
                    if let ConstBinding::Name(name) = &decl.binding {
                        self.local_names.insert(name.clone());
                    }
                }
                ItemKind::Import(decl) => {
                    for spec in &decl.specifiers {
                        let name = spec.alias.as_ref().unwrap_or(&spec.name);
                        self.local_names.insert(name.clone());
                    }
                }
                ItemKind::ForBlock(block) => {
                    for func in &block.functions {
                        self.local_names.insert(func.name.clone());
                    }
                }
                _ => {}
            }
        }

        for (i, item) in program.items.iter().enumerate() {
            if i > 0 {
                self.newline();
            }
            self.emit_item(item);
            self.newline();
        }

        // Prepend structural equality helper if any == or != was used
        if self.needs_deep_equal {
            let helper = concat!(
                "function __floeEq(a: unknown, b: unknown): boolean {\n",
                "  if (a === b) return true;\n",
                "  if (a == null || b == null) return false;\n",
                "  if (typeof a !== \"object\" || typeof b !== \"object\") return false;\n",
                "  const ka = Object.keys(a as object);\n",
                "  const kb = Object.keys(b as object);\n",
                "  if (ka.length !== kb.length) return false;\n",
                "  return ka.every((k) => __floeEq((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));\n",
                "}\n\n",
            );
            self.output = format!("{helper}{}", self.output);
        }

        let dts = self.generate_dts(program);

        CodegenOutput {
            code: self.output,
            has_jsx: self.has_jsx,
            dts,
        }
    }

    /// Check if an expression contains `?` (Unwrap) at any level,
    /// and return true if the const should use Result unwrapping.
    fn expr_has_unwrap(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Unwrap(_) => true,
            ExprKind::Await(inner) => Self::expr_has_unwrap(inner),
            ExprKind::Pipe { left, right } => {
                Self::expr_has_unwrap(left) || Self::expr_has_unwrap(right)
            }
            _ => false,
        }
    }

    /// Flatten a chain of `Unwrap(Pipe { left: Unwrap(Pipe { ... }), right })` into
    /// sequential steps. This enables emitting clean `const _rN = ...; if (!_rN.ok) return _rN;`
    /// instead of deeply nested IIFEs.
    fn flatten_pipe_unwrap_chain(expr: &Expr) -> Vec<PipeStep> {
        let mut steps = Vec::new();
        Self::collect_pipe_steps(expr, &mut steps);
        steps
    }

    fn collect_pipe_steps(expr: &Expr, steps: &mut Vec<PipeStep>) {
        match &expr.kind {
            // Unwrap(Pipe { left, right }) → recurse into left, then add right as a pipe step with unwrap
            ExprKind::Unwrap(inner) => match &inner.kind {
                ExprKind::Pipe { left, right } => {
                    Self::collect_pipe_steps(left, steps);
                    steps.push(PipeStep {
                        expr: (**right).clone(),
                        unwrap: true,
                        is_await: false,
                        is_pipe: true,
                    });
                }
                ExprKind::Await(await_inner) => match &await_inner.kind {
                    ExprKind::Pipe { left, right } => {
                        Self::collect_pipe_steps(left, steps);
                        steps.push(PipeStep {
                            expr: (**right).clone(),
                            unwrap: true,
                            is_await: true,
                            is_pipe: true,
                        });
                    }
                    _ => {
                        // await expr? → base step
                        steps.push(PipeStep {
                            expr: (**await_inner).clone(),
                            unwrap: true,
                            is_await: true,
                            is_pipe: false,
                        });
                    }
                },
                _ => {
                    // Simple unwrap without pipe
                    steps.push(PipeStep {
                        expr: (**inner).clone(),
                        unwrap: true,
                        is_await: false,
                        is_pipe: false,
                    });
                }
            },
            // Await(Unwrap(...)) → unwrap the inner with await flag
            ExprKind::Await(inner) if matches!(inner.kind, ExprKind::Unwrap(_)) => {
                if let ExprKind::Unwrap(unwrap_inner) = &inner.kind {
                    match &unwrap_inner.kind {
                        ExprKind::Pipe { left, right } => {
                            Self::collect_pipe_steps(left, steps);
                            steps.push(PipeStep {
                                expr: (**right).clone(),
                                unwrap: true,
                                is_await: true,
                                is_pipe: true,
                            });
                        }
                        _ => {
                            steps.push(PipeStep {
                                expr: (**unwrap_inner).clone(),
                                unwrap: true,
                                is_await: true,
                                is_pipe: false,
                            });
                        }
                    }
                }
            }
            // Pipe without unwrap at this level
            ExprKind::Pipe { left, right } => {
                Self::collect_pipe_steps(left, steps);
                steps.push(PipeStep {
                    expr: (**right).clone(),
                    unwrap: false,
                    is_await: false,
                    is_pipe: true,
                });
            }
            // Base expression (no pipe, no unwrap)
            _ => {
                steps.push(PipeStep {
                    expr: expr.clone(),
                    unwrap: false,
                    is_await: false,
                    is_pipe: false,
                });
            }
        }
    }

    // ── Items ────────────────────────────────────────────────────

    fn emit_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Import(decl) => self.emit_import(decl),
            ItemKind::Const(decl) => self.emit_const(decl),
            ItemKind::Function(decl) => self.emit_function(decl),
            ItemKind::TypeDecl(decl) => self.emit_type_decl(decl),
            ItemKind::ForBlock(block) => self.emit_for_block(block),
            ItemKind::TraitDecl(_) => {
                // Traits are erased at compile time — emit nothing
            }
            ItemKind::TestBlock(block) => self.emit_test_block(block),
            ItemKind::Expr(expr) => {
                self.emit_indent();
                self.emit_expr(expr);
                self.push(";");
            }
        }
    }

    // ── Import ───────────────────────────────────────────────────

    fn emit_import(&mut self, decl: &ImportDecl) {
        self.emit_indent();
        if decl.specifiers.is_empty() && decl.for_specifiers.is_empty() {
            // Bare import: expand to named imports if we have resolved exports
            if let Some(resolved) = self.resolved_imports.get(&decl.source) {
                let mut names: Vec<String> = Vec::new();
                for func in &resolved.function_decls {
                    if func.exported {
                        names.push(func.name.clone());
                    }
                }
                for block in &resolved.for_blocks {
                    for func in &block.functions {
                        if func.exported {
                            names.push(func.name.clone());
                        }
                    }
                }
                for name in &resolved.const_names {
                    names.push(name.clone());
                }
                if names.is_empty() {
                    self.push(&format!("import \"{}\";", decl.source));
                } else {
                    // Always alias bare-import names to avoid TDZ conflicts
                    // (e.g., `const remaining = todos |> remaining` would fail
                    // without aliasing because JS const shadows the import)
                    let specifiers: Vec<String> = names
                        .iter()
                        .map(|name| {
                            let alias = format!("__{name}");
                            self.import_aliases.insert(name.clone(), alias.clone());
                            format!("{name} as {alias}")
                        })
                        .collect();
                    self.push(&format!(
                        "import {{ {} }} from \"{}\";",
                        specifiers.join(", "),
                        decl.source
                    ));
                }
            } else {
                self.push(&format!("import \"{}\";", decl.source));
            }
        } else {
            // Determine which specifiers are type-only (not runtime values)
            let type_only_names: std::collections::HashSet<String> =
                if let Some(resolved) = self.resolved_imports.get(&decl.source) {
                    decl.specifiers
                        .iter()
                        .filter(|spec| {
                            resolved.type_decls.iter().any(|t| t.name == spec.name)
                                && !resolved.function_decls.iter().any(|f| f.name == spec.name)
                                && !resolved.const_names.contains(&spec.name)
                        })
                        .map(|spec| spec.name.clone())
                        .collect()
                } else {
                    std::collections::HashSet::new()
                };
            self.push("import { ");
            let mut first = true;
            for spec in &decl.specifiers {
                if !first {
                    self.push(", ");
                }
                first = false;
                if type_only_names.contains(&spec.name) {
                    self.push("type ");
                }
                self.push(&spec.name);
                if let Some(alias) = &spec.alias {
                    self.push(" as ");
                    self.push(alias);
                }
            }
            // Expand `for Type` specifiers into concrete function names
            let for_func_names = self.resolve_for_import_names(decl);
            for name in &for_func_names {
                if !first {
                    self.push(", ");
                }
                first = false;
                self.push(name);
            }
            self.push(&format!(" }} from \"{}\";", decl.source));
        }
    }

    // ── Const ────────────────────────────────────────────────────

    fn emit_const(&mut self, decl: &ConstDecl) {
        // Handle `const x = expr?` → Result unwrap with early return
        // For chained pipes with `?`: flatten into sequential _rN steps
        if Self::expr_has_unwrap(&decl.value) {
            let steps = Self::flatten_pipe_unwrap_chain(&decl.value);

            // Track the name of the last temp var for the final binding
            let mut last_temp = String::new();
            let mut last_had_unwrap = false;

            for (i, step) in steps.iter().enumerate() {
                let temp = format!("_r{}", self.unwrap_counter);
                self.unwrap_counter += 1;

                // Emit the step expression into a buffer to detect async IIFEs
                let step_code = if step.is_pipe {
                    let left_expr = if last_had_unwrap {
                        Expr {
                            kind: ExprKind::Identifier(format!("{last_temp}.value")),
                            span: step.expr.span,
                        }
                    } else {
                        Expr {
                            kind: ExprKind::Identifier(last_temp.clone()),
                            span: step.expr.span,
                        }
                    };
                    let mut sub = self.sub_codegen();
                    sub.emit_pipe(&left_expr, &step.expr);
                    if sub.needs_deep_equal {
                        self.needs_deep_equal = true;
                    }
                    sub.output
                } else {
                    let mut sub = self.sub_codegen();
                    sub.emit_expr(&step.expr);
                    if sub.needs_deep_equal {
                        self.needs_deep_equal = true;
                    }
                    sub.output
                };

                // Determine if we need `await`: explicit from source or async IIFE from stdlib
                let needs_await = step.is_await || step_code.starts_with("(async ");

                self.emit_indent();
                if needs_await {
                    self.push(&format!("const {temp} = await "));
                } else {
                    self.push(&format!("const {temp} = "));
                }
                self.push(&step_code);
                self.push(";");
                self.newline();

                if step.unwrap {
                    self.emit_indent();
                    self.push(&format!("if (!{temp}.ok) return {temp};"));
                    self.newline();
                    last_had_unwrap = true;
                } else {
                    last_had_unwrap = false;
                }
                last_temp = temp;

                // After the last step with unwrap, if this is the final step
                // or if i is last, emit the final binding
                if i == steps.len() - 1 {
                    let value_expr = if last_had_unwrap {
                        format!("{last_temp}.value")
                    } else {
                        last_temp.clone()
                    };

                    self.emit_indent();
                    if decl.exported {
                        self.push("export ");
                    }
                    self.push("const ");
                    match &decl.binding {
                        ConstBinding::Name(name) => self.push(name),
                        ConstBinding::Array(names) | ConstBinding::Tuple(names) => {
                            self.push("[");
                            self.push(&names.join(", "));
                            self.push("]");
                        }
                        ConstBinding::Object(names) => {
                            self.push("{ ");
                            self.push(&names.join(", "));
                            self.push(" }");
                        }
                    }
                    self.push(&format!(" = {value_expr};"));
                }
            }
            return;
        }

        self.emit_indent();
        if decl.exported {
            self.push("export ");
        }
        self.push("const ");

        match &decl.binding {
            ConstBinding::Name(name) => self.push(name),
            ConstBinding::Array(names) => {
                self.push("[");
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(name);
                }
                self.push("]");
            }
            ConstBinding::Tuple(names) => {
                // Tuple destructuring: const (a, b) = ... → const [a, b] = ...
                self.push("[");
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(name);
                }
                self.push("]");
            }
            ConstBinding::Object(names) => {
                self.push("{ ");
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(name);
                }
                self.push(" }");
            }
        }

        if let Some(type_ann) = &decl.type_ann {
            self.push(": ");
            self.emit_type_expr(type_ann);
        }

        self.push(" = ");
        self.emit_expr(&decl.value);
        self.push(";");
    }

    // ── Function ─────────────────────────────────────────────────

    fn emit_function(&mut self, decl: &FunctionDecl) {
        // `fn name = expr` — derived function binding, emit as `const name = expr;`
        if decl.params.is_empty()
            && decl.return_type.is_none()
            && !matches!(decl.body.kind, ExprKind::Block(_))
        {
            self.emit_indent();
            if decl.exported {
                self.push("export ");
            }
            self.push("const ");
            self.push(&decl.name);
            self.push(" = ");
            self.emit_expr(&decl.body);
            self.push(";");
            return;
        }

        self.emit_indent();
        if decl.exported {
            self.push("export ");
        }
        if decl.async_fn {
            self.push("async ");
        }
        self.push("function ");
        self.push(&decl.name);
        if !decl.type_params.is_empty() {
            self.push("<");
            self.push(&decl.type_params.join(", "));
            self.push(">");
        }
        self.push("(");
        self.emit_params(&decl.params);
        self.push(")");

        // Check if return type is unit/void — if so, no implicit return needed
        let is_unit_return = decl.return_type.as_ref().is_some_and(
            |rt| matches!(&rt.kind, TypeExprKind::Named { name, .. } if name == type_names::UNIT),
        );

        if let Some(ret) = &decl.return_type {
            self.push(": ");
            self.emit_type_expr(ret);
        }

        self.push(" ");
        if is_unit_return {
            self.emit_block_expr(&decl.body);
        } else {
            self.emit_block_expr_with_return(&decl.body);
        }
    }

    fn emit_params(&mut self, params: &[Param]) {
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.emit_param(param);
        }
    }

    fn emit_param(&mut self, param: &Param) {
        match &param.destructure {
            Some(ParamDestructure::Object(fields)) => {
                self.push("{ ");
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(field);
                }
                self.push(" }");
            }
            Some(ParamDestructure::Array(fields)) => {
                self.push("[");
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(field);
                }
                self.push("]");
            }
            None => {
                self.push(&param.name);
            }
        }
        if let Some(type_ann) = &param.type_ann {
            self.push(": ");
            self.emit_type_expr(type_ann);
        }
        if let Some(default) = &param.default {
            self.push(" = ");
            self.emit_expr(default);
        }
    }

    // ── For Blocks ────────────────────────────────────────────────

    fn emit_for_block(&mut self, block: &ForBlock) {
        for (i, func) in block.functions.iter().enumerate() {
            if i > 0 {
                self.newline();
            }
            self.emit_for_block_function(func, &block.type_name);
        }
    }

    fn emit_for_block_function(&mut self, func: &FunctionDecl, for_type: &TypeExpr) {
        self.emit_indent();
        if func.exported {
            self.push("export ");
        }
        if func.async_fn {
            self.push("async ");
        }
        self.push("function ");
        self.push(&func.name);
        self.push("(");

        // Emit parameters, replacing `self` with the for block's type
        for (i, param) in func.params.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.push(&param.name);
            if param.name == "self" {
                self.push(": ");
                self.emit_type_expr(for_type);
            } else if let Some(type_ann) = &param.type_ann {
                self.push(": ");
                self.emit_type_expr(type_ann);
            }
            if let Some(default) = &param.default {
                self.push(" = ");
                self.emit_expr(default);
            }
        }

        self.push(")");

        let is_unit_return = func.return_type.as_ref().is_some_and(
            |rt| matches!(&rt.kind, TypeExprKind::Named { name, .. } if name == type_names::UNIT),
        );

        if let Some(ret) = &func.return_type {
            self.push(": ");
            self.emit_type_expr(ret);
        }

        self.push(" ");
        if is_unit_return {
            self.emit_block_expr(&func.body);
        } else {
            self.emit_block_expr_with_return(&func.body);
        }
    }

    // ── Test Blocks ──────────────────────────────────────────────

    fn emit_test_block(&mut self, block: &TestBlock) {
        // In production mode, skip test blocks entirely
        if !self.test_mode {
            return;
        }

        // Emit as a self-executing test function
        self.emit_indent();
        self.push(&format!("// test: {}", escape_string(&block.name)));
        self.newline();
        self.emit_indent();
        self.push("(function() {");
        self.newline();
        self.indent += 1;

        self.emit_indent();
        self.push(&format!(
            "const __testName = \"{}\";",
            escape_string(&block.name)
        ));
        self.newline();

        self.emit_indent();
        self.push("let __passed = 0;");
        self.newline();
        self.emit_indent();
        self.push("let __failed = 0;");
        self.newline();

        for stmt in &block.body {
            match stmt {
                TestStatement::Assert(expr, _) => {
                    self.emit_indent();
                    self.push("try { if (!(");
                    self.emit_expr(expr);
                    self.push(")) { __failed++; console.error(`  FAIL: ");
                    // Emit the assertion source as a string
                    let expr_str = self.expr_to_string(expr);
                    self.push(&escape_string(&expr_str));
                    self.push("`); } else { __passed++; } } catch (e) { __failed++; console.error(`  FAIL: ");
                    self.push(&escape_string(&expr_str));
                    self.push("`, e); }");
                    self.newline();
                }
                TestStatement::Expr(expr) => {
                    self.emit_indent();
                    self.emit_expr(expr);
                    self.push(";");
                    self.newline();
                }
            }
        }

        self.emit_indent();
        self.push("if (__failed > 0) { console.error(`FAIL ${__testName}: ${__passed} passed, ${__failed} failed`); process.exitCode = 1; }");
        self.newline();
        self.emit_indent();
        self.push("else { console.log(`PASS ${__testName}: ${__passed} passed`); }");
        self.newline();

        self.indent -= 1;
        self.emit_indent();
        self.push("})();");
    }

    // ── Type Declarations ────────────────────────────────────────

    fn emit_type_decl(&mut self, decl: &TypeDecl) {
        self.emit_indent();
        if decl.exported {
            self.push("export ");
        }
        self.push("type ");
        self.push(&decl.name);

        if !decl.type_params.is_empty() {
            self.push("<");
            for (i, tp) in decl.type_params.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(tp);
            }
            self.push(">");
        }

        self.push(" = ");

        match &decl.def {
            TypeDef::Record(entries) => {
                self.emit_record_type_entries(entries);
            }
            TypeDef::Union(variants) => {
                self.emit_union_type(variants);
            }
            TypeDef::StringLiteralUnion(variants) => {
                self.emit_string_literal_union_type(variants);
            }
            TypeDef::Alias(type_expr) => {
                // Opaque types erase to their underlying type
                self.emit_type_expr(type_expr);
            }
        }

        self.push(";");

        // Emit derived trait implementations
        if !decl.deriving.is_empty()
            && let TypeDef::Record(_) = &decl.def
        {
            let fields = decl.def.record_fields();
            for trait_name in &decl.deriving {
                self.newline();
                self.newline();
                if trait_name.as_str() == "Display" {
                    self.emit_derived_display(&decl.name, &fields);
                }
            }
        }
    }

    fn emit_derived_display(&mut self, type_name: &str, fields: &[&RecordField]) {
        self.emit_indent();
        self.push(&format!("function display(self: {type_name}): string {{"));
        self.newline();
        self.indent += 1;
        self.emit_indent();
        self.push("return `");
        self.push(type_name);
        self.push("(");
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.push(&format!("{}: ${{self.{}}}", field.name, field.name));
        }
        self.push(")`;");
        self.newline();
        self.indent -= 1;
        self.emit_indent();
        self.push("}");
    }

    fn emit_record_type_entries(&mut self, entries: &[RecordEntry]) {
        let spreads: Vec<&RecordSpread> = entries.iter().filter_map(|e| e.as_spread()).collect();
        let fields: Vec<&RecordField> = entries.iter().filter_map(|e| e.as_field()).collect();

        // Emit spreads as intersection types
        for spread in &spreads {
            self.push(&spread.type_name);
            if !fields.is_empty() || spread != spreads.last().unwrap() {
                self.push(" & ");
            }
        }

        if !fields.is_empty() || spreads.is_empty() {
            self.emit_record_type_fields(&fields);
        }
    }

    fn emit_record_type_fields(&mut self, fields: &[&RecordField]) {
        self.push("{ ");
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.push("; ");
            }
            self.push(&field.name);
            self.push(": ");
            self.emit_type_expr(&field.type_ann);
        }
        self.push(" }");
    }

    fn emit_record_type(&mut self, fields: &[RecordField]) {
        self.push("{ ");
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.push("; ");
            }
            self.push(&field.name);
            self.push(": ");
            self.emit_type_expr(&field.type_ann);
        }
        self.push(" }");
    }

    fn emit_union_type(&mut self, variants: &[Variant]) {
        for (i, variant) in variants.iter().enumerate() {
            if i > 0 {
                self.push(" | ");
            }

            if variant.fields.is_empty() {
                // Simple variant: `{ tag: "Home" }`
                self.push(&format!("{{ {TAG_FIELD}: \"{}\" }}", variant.name));
            } else {
                // Variant with fields: `{ tag: "Profile"; id: string }`
                self.push(&format!("{{ {TAG_FIELD}: \"{}\"", variant.name));
                for field in &variant.fields {
                    self.push("; ");
                    if let Some(name) = &field.name {
                        self.push(name);
                    } else {
                        self.push(VALUE_FIELD);
                    }
                    self.push(": ");
                    self.emit_type_expr(&field.type_ann);
                }
                self.push(" }");
            }
        }
    }

    fn emit_string_literal_union_type(&mut self, variants: &[String]) {
        for (i, variant) in variants.iter().enumerate() {
            if i > 0 {
                self.push(" | ");
            }
            self.push(&format!("\"{}\"", escape_string(variant)));
        }
    }

    // ── Type Expressions ─────────────────────────────────────────

    fn emit_type_expr(&mut self, type_expr: &TypeExpr) {
        match &type_expr.kind {
            TypeExprKind::Named {
                name, type_args, ..
            } => {
                // Option<T> becomes T | undefined
                if name == type_names::OPTION && type_args.len() == 1 {
                    self.emit_type_expr(&type_args[0]);
                    self.push(" | undefined");
                    return;
                }
                // Result<T, E> becomes { ok: true; value: T } | { ok: false; error: E }
                if name == type_names::RESULT && type_args.len() == 2 {
                    self.push(&format!("{{ {OK_FIELD}: true; {VALUE_FIELD}: "));
                    self.emit_type_expr(&type_args[0]);
                    self.push(&format!(" }} | {{ {OK_FIELD}: false; {ERROR_FIELD}: "));
                    self.emit_type_expr(&type_args[1]);
                    self.push(" }");
                    return;
                }

                // Unit type () becomes void in TypeScript
                if name == type_names::UNIT {
                    self.push("void");
                    return;
                }

                self.push(name);
                if !type_args.is_empty() {
                    self.push("<");
                    for (i, arg) in type_args.iter().enumerate() {
                        if i > 0 {
                            self.push(", ");
                        }
                        self.emit_type_expr(arg);
                    }
                    self.push(">");
                }
            }
            TypeExprKind::Record(fields) => {
                self.emit_record_type(fields);
            }
            TypeExprKind::Function {
                params,
                return_type,
            } => {
                self.push("(");
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(&format!("_p{i}: "));
                    self.emit_type_expr(param);
                }
                self.push(") => ");
                self.emit_type_expr(return_type);
            }
            TypeExprKind::Array(inner) => {
                self.emit_type_expr(inner);
                self.push("[]");
            }
            TypeExprKind::Tuple(types) => {
                self.push("readonly [");
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.emit_type_expr(t);
                }
                self.push("]");
            }
        }
    }

    /// Resolve `for Type` import specifiers to concrete function names.
    fn resolve_for_import_names(&self, decl: &ImportDecl) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(resolved) = self.resolved_imports.get(&decl.source) {
            for for_spec in &decl.for_specifiers {
                for block in &resolved.for_blocks {
                    let base_type_name = match &block.type_name.kind {
                        TypeExprKind::Named { name, .. } => name.clone(),
                        _ => continue,
                    };
                    if base_type_name == for_spec.type_name {
                        for func in &block.functions {
                            if func.exported {
                                names.push(func.name.clone());
                            }
                        }
                    }
                }
            }
        }
        names
    }

    // ── Declaration Stub Generation (.d.ts) ───────────────────────

    /// Generate a `.d.ts` declaration stub from the program AST.
    /// Only emits exported type declarations, function signatures, and const declarations.
    fn generate_dts(&self, program: &Program) -> String {
        let mut out = String::new();
        let mut first = true;

        for item in &program.items {
            match &item.kind {
                ItemKind::Import(decl) => {
                    if !first {
                        out.push('\n');
                    }
                    first = false;
                    self.emit_dts_import(&mut out, decl);
                }
                ItemKind::TypeDecl(decl) => {
                    if !first {
                        out.push('\n');
                    }
                    first = false;
                    self.emit_dts_type_decl(&mut out, decl);
                }
                ItemKind::Function(decl) => {
                    if !decl.exported {
                        continue;
                    }
                    if !first {
                        out.push('\n');
                    }
                    first = false;
                    self.emit_dts_function(&mut out, decl);
                }
                ItemKind::Const(decl) => {
                    if !decl.exported {
                        continue;
                    }
                    if !first {
                        out.push('\n');
                    }
                    first = false;
                    self.emit_dts_const(&mut out, decl);
                }
                ItemKind::ForBlock(block) => {
                    for func in &block.functions {
                        if !func.exported {
                            continue;
                        }
                        if !first {
                            out.push('\n');
                        }
                        first = false;
                        self.emit_dts_for_block_function(&mut out, func, &block.type_name);
                    }
                }
                // Traits, tests, and expressions don't produce declarations
                ItemKind::TraitDecl(_) | ItemKind::TestBlock(_) | ItemKind::Expr(_) => {}
            }
        }

        if !out.is_empty() {
            out.push('\n');
        }
        out
    }

    fn emit_dts_import(&self, out: &mut String, decl: &ImportDecl) {
        if decl.specifiers.is_empty() && decl.for_specifiers.is_empty() {
            // Bare import: expand to type-only named imports if we have resolved exports
            if let Some(resolved) = self.resolved_imports.get(&decl.source) {
                let mut type_names: Vec<String> = Vec::new();
                for td in &resolved.type_decls {
                    if td.exported {
                        type_names.push(td.name.clone());
                    }
                }
                let mut value_names: Vec<String> = Vec::new();
                for func in &resolved.function_decls {
                    if func.exported {
                        value_names.push(func.name.clone());
                    }
                }
                for block in &resolved.for_blocks {
                    for func in &block.functions {
                        if func.exported {
                            value_names.push(func.name.clone());
                        }
                    }
                }
                for name in &resolved.const_names {
                    value_names.push(name.clone());
                }

                let mut specs: Vec<String> = Vec::new();
                for name in &type_names {
                    specs.push(format!("type {name}"));
                }
                for name in &value_names {
                    specs.push(name.clone());
                }

                if !specs.is_empty() {
                    out.push_str(&format!(
                        "import {{ {} }} from \"{}\";",
                        specs.join(", "),
                        decl.source
                    ));
                }
            }
        } else {
            // Named imports: determine which are type-only
            let type_only_names: HashSet<String> =
                if let Some(resolved) = self.resolved_imports.get(&decl.source) {
                    decl.specifiers
                        .iter()
                        .filter(|spec| {
                            resolved.type_decls.iter().any(|t| t.name == spec.name)
                                && !resolved.function_decls.iter().any(|f| f.name == spec.name)
                                && !resolved.const_names.contains(&spec.name)
                        })
                        .map(|spec| spec.name.clone())
                        .collect()
                } else {
                    HashSet::new()
                };

            out.push_str("import { ");
            let mut first = true;
            for spec in &decl.specifiers {
                if !first {
                    out.push_str(", ");
                }
                first = false;
                if type_only_names.contains(&spec.name) {
                    out.push_str("type ");
                }
                out.push_str(&spec.name);
                if let Some(alias) = &spec.alias {
                    out.push_str(" as ");
                    out.push_str(alias);
                }
            }
            // Expand `for Type` specifiers
            let for_func_names = self.resolve_for_import_names(decl);
            for name in &for_func_names {
                if !first {
                    out.push_str(", ");
                }
                first = false;
                out.push_str(name);
            }
            out.push_str(&format!(" }} from \"{}\";", decl.source));
        }
    }

    fn emit_dts_type_decl(&self, out: &mut String, decl: &TypeDecl) {
        // Emit the type declaration only (no derived trait implementations)
        if decl.exported {
            out.push_str("export ");
        }
        out.push_str("type ");
        out.push_str(&decl.name);

        if !decl.type_params.is_empty() {
            out.push('<');
            for (i, tp) in decl.type_params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(tp);
            }
            out.push('>');
        }

        out.push_str(" = ");

        let mut cg = self.sub_codegen();
        match &decl.def {
            TypeDef::Record(entries) => cg.emit_record_type_entries(entries),
            TypeDef::Union(variants) => cg.emit_union_type(variants),
            TypeDef::StringLiteralUnion(variants) => cg.emit_string_literal_union_type(variants),
            TypeDef::Alias(type_expr) => cg.emit_type_expr(type_expr),
        }
        out.push_str(&cg.output);
        out.push(';');

        // For derived Display on record types, emit the function declaration
        if !decl.deriving.is_empty()
            && let TypeDef::Record(_) = &decl.def
        {
            for trait_name in &decl.deriving {
                if trait_name.as_str() == "Display" {
                    out.push_str(&format!(
                        "\nexport declare function display(self: {}): string;",
                        decl.name
                    ));
                }
            }
        }
    }

    fn emit_dts_function(&self, out: &mut String, decl: &FunctionDecl) {
        // `fn name = expr` — derived function binding
        if decl.params.is_empty()
            && decl.return_type.is_none()
            && !matches!(decl.body.kind, ExprKind::Block(_))
        {
            out.push_str(&format!("export declare const {}: any;", decl.name));
            return;
        }

        out.push_str("export declare ");
        if decl.async_fn {
            out.push_str("async ");
        }
        out.push_str("function ");
        out.push_str(&decl.name);
        if !decl.type_params.is_empty() {
            out.push('<');
            out.push_str(&decl.type_params.join(", "));
            out.push('>');
        }
        out.push('(');
        let mut cg = self.sub_codegen();
        cg.emit_params(&decl.params);
        out.push_str(&cg.output);
        out.push(')');

        if let Some(ret) = &decl.return_type {
            out.push_str(": ");
            let mut cg = self.sub_codegen();
            cg.emit_type_expr(ret);
            out.push_str(&cg.output);
        }
        out.push(';');
    }

    fn emit_dts_const(&self, out: &mut String, decl: &ConstDecl) {
        match &decl.binding {
            ConstBinding::Name(name) => {
                out.push_str("export declare const ");
                out.push_str(name);
                if let Some(type_ann) = &decl.type_ann {
                    out.push_str(": ");
                    let mut cg = self.sub_codegen();
                    cg.emit_type_expr(type_ann);
                    out.push_str(&cg.output);
                } else {
                    out.push_str(": any");
                }
                out.push(';');
            }
            ConstBinding::Array(names) | ConstBinding::Tuple(names) => {
                for name in names {
                    out.push_str(&format!("export declare const {name}: any;"));
                }
            }
            ConstBinding::Object(names) => {
                for name in names {
                    out.push_str(&format!("export declare const {name}: any;"));
                }
            }
        }
    }

    fn emit_dts_for_block_function(
        &self,
        out: &mut String,
        func: &FunctionDecl,
        for_type: &TypeExpr,
    ) {
        out.push_str("export declare ");
        if func.async_fn {
            out.push_str("async ");
        }
        out.push_str("function ");
        out.push_str(&func.name);
        out.push('(');

        for (i, param) in func.params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&param.name);
            if param.name == "self" {
                out.push_str(": ");
                let mut cg = self.sub_codegen();
                cg.emit_type_expr(for_type);
                out.push_str(&cg.output);
            } else if let Some(type_ann) = &param.type_ann {
                out.push_str(": ");
                let mut cg = self.sub_codegen();
                cg.emit_type_expr(type_ann);
                out.push_str(&cg.output);
            }
        }

        out.push(')');

        if let Some(ret) = &func.return_type {
            out.push_str(": ");
            let mut cg = self.sub_codegen();
            cg.emit_type_expr(ret);
            out.push_str(&cg.output);
        }
        out.push(';');
    }

    // ── Output helpers ───────────────────────────────────────────

    fn push(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn newline(&mut self) {
        self.output.push('\n');
    }

    fn emit_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
    }

    fn expr_to_string(&self, expr: &Expr) -> String {
        let mut cg = self.sub_codegen();
        cg.emit_expr(expr);
        cg.output
    }

    /// Create a sub-codegen that shares type info but has its own output buffer.
    fn sub_codegen(&self) -> Codegen {
        Codegen {
            output: String::new(),
            indent: 0,
            has_jsx: false,
            needs_deep_equal: false,
            unwrap_counter: 0,
            stdlib: StdlibRegistry::new(),
            unit_variants: self.unit_variants.clone(),
            variant_info: self.variant_info.clone(),
            local_names: self.local_names.clone(),
            expr_types: self.expr_types.clone(),
            resolved_imports: self.resolved_imports.clone(),
            import_aliases: self.import_aliases.clone(),
            test_mode: self.test_mode,
        }
    }
}

impl Default for Codegen {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ──────────────────────────────────────────────────────

/// Expand a codegen template like `$0.map($1)` with actual arg strings.
pub(super) fn expand_codegen_template(template: &str, args: &[String]) -> String {
    let mut result = template.to_string();
    // Replace in reverse order so $10 doesn't get matched by $1
    for (i, arg) in args.iter().enumerate().rev() {
        result = result.replace(&format!("${i}"), arg);
    }
    result
}

pub(super) fn binop_str(op: BinOp) -> &'static str {
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

pub(super) fn unaryop_str(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
    }
}

pub(super) fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub(super) fn has_placeholder_arg(args: &[Arg]) -> bool {
    args.iter().any(|a| match a {
        Arg::Positional(expr) => matches!(expr.kind, ExprKind::Placeholder),
        Arg::Named { value, .. } => matches!(value.kind, ExprKind::Placeholder),
    })
}
