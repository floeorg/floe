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

/// Code generation result: the emitted TypeScript source and whether it contains JSX.
pub struct CodegenOutput {
    pub code: String,
    pub has_jsx: bool,
}

/// The Floe code generator. Emits clean, readable TypeScript / TSX.
pub struct Codegen {
    output: String,
    indent: usize,
    has_jsx: bool,
    needs_deep_equal: bool,
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
                            .filter_map(|f| f.name.clone())
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
                                .filter_map(|f| f.name.clone())
                                .collect();
                            if variant.fields.is_empty() {
                                self.unit_variants.insert(variant.name.clone());
                            }
                            self.variant_info
                                .insert(variant.name.clone(), (decl.name.clone(), field_names));
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
                "function __zenEq(a: unknown, b: unknown): boolean {\n",
                "  if (a === b) return true;\n",
                "  if (a == null || b == null) return false;\n",
                "  if (typeof a !== \"object\" || typeof b !== \"object\") return false;\n",
                "  const ka = Object.keys(a as object);\n",
                "  const kb = Object.keys(b as object);\n",
                "  if (ka.length !== kb.length) return false;\n",
                "  return ka.every((k) => __zenEq((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));\n",
                "}\n\n",
            );
            self.output = format!("{helper}{}", self.output);
        }

        CodegenOutput {
            code: self.output,
            has_jsx: self.has_jsx,
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
        if decl.specifiers.is_empty() {
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
            self.push("import { ");
            for (i, spec) in decl.specifiers.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(&spec.name);
                if let Some(alias) = &spec.alias {
                    self.push(" as ");
                    self.push(alias);
                }
            }
            self.push(&format!(" }} from \"{}\";", decl.source));
        }
    }

    // ── Const ────────────────────────────────────────────────────

    fn emit_const(&mut self, decl: &ConstDecl) {
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
        self.emit_indent();
        if decl.exported {
            self.push("export ");
        }
        if decl.async_fn {
            self.push("async ");
        }
        self.push("function ");
        self.push(&decl.name);
        self.push("(");
        self.emit_params(&decl.params);
        self.push(")");

        // Check if return type is unit/void — if so, no implicit return needed
        let is_unit_return = decl
            .return_type
            .as_ref()
            .is_some_and(|rt| matches!(&rt.kind, TypeExprKind::Named { name, .. } if name == "()"));

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
            self.push(&param.name);
            if let Some(type_ann) = &param.type_ann {
                self.push(": ");
                self.emit_type_expr(type_ann);
            }
            if let Some(default) = &param.default {
                self.push(" = ");
                self.emit_expr(default);
            }
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

        let is_unit_return = func
            .return_type
            .as_ref()
            .is_some_and(|rt| matches!(&rt.kind, TypeExprKind::Named { name, .. } if name == "()"));

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
            TypeDef::Record(fields) => {
                self.emit_record_type(fields);
            }
            TypeDef::Union(variants) => {
                self.emit_union_type(&decl.name, variants);
            }
            TypeDef::Alias(type_expr) => {
                // Brand and opaque types erase to their underlying type
                self.emit_type_expr(type_expr);
            }
        }

        self.push(";");
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

    fn emit_union_type(&mut self, _parent_name: &str, variants: &[Variant]) {
        for (i, variant) in variants.iter().enumerate() {
            if i > 0 {
                self.push(" | ");
            }

            if variant.fields.is_empty() {
                // Simple variant: `{ tag: "Home" }`
                self.push(&format!("{{ tag: \"{}\" }}", variant.name));
            } else {
                // Variant with fields: `{ tag: "Profile"; id: string }`
                self.push(&format!("{{ tag: \"{}\"", variant.name));
                for field in &variant.fields {
                    self.push("; ");
                    if let Some(name) = &field.name {
                        self.push(name);
                    } else {
                        self.push("value");
                    }
                    self.push(": ");
                    self.emit_type_expr(&field.type_ann);
                }
                self.push(" }");
            }
        }
    }

    // ── Type Expressions ─────────────────────────────────────────

    fn emit_type_expr(&mut self, type_expr: &TypeExpr) {
        match &type_expr.kind {
            TypeExprKind::Named { name, type_args } => {
                // Brand<T, "Name"> erases to T
                if name == "Brand" && type_args.len() == 2 {
                    self.emit_type_expr(&type_args[0]);
                    return;
                }
                // Option<T> becomes T | undefined
                if name == "Option" && type_args.len() == 1 {
                    self.emit_type_expr(&type_args[0]);
                    self.push(" | undefined");
                    return;
                }
                // Result<T, E> becomes { ok: true; value: T } | { ok: false; error: E }
                if name == "Result" && type_args.len() == 2 {
                    self.push("{ ok: true; value: ");
                    self.emit_type_expr(&type_args[0]);
                    self.push(" } | { ok: false; error: ");
                    self.emit_type_expr(&type_args[1]);
                    self.push(" }");
                    return;
                }

                // Unit type () becomes void in TypeScript
                if name == "()" {
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
