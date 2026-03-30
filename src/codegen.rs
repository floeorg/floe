mod expr;
mod jsx;
mod match_emit;
mod parse_mock;
mod pipes;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use crate::parser::ast::*;
use crate::resolve::ResolvedImports;
use crate::stdlib::StdlibRegistry;
use crate::type_layout;
use crate::type_layout::{ERROR_FIELD, OK_FIELD, TAG_FIELD, VALUE_FIELD};

/// Produce a mangled name for a for-block function: `TypeName__funcName`.
/// Generic types are flattened: `Array<User>` → `Array_User`, `Map<string, number>` → `Map_string_number`.
pub fn for_block_fn_name(type_expr: &TypeExpr, fn_name: &str) -> String {
    let type_prefix = mangle_type_name(type_expr);
    format!("{type_prefix}__{fn_name}")
}

/// Mangle a type expression into a valid identifier fragment.
fn mangle_type_name(type_expr: &TypeExpr) -> String {
    match &type_expr.kind {
        TypeExprKind::Named {
            name, type_args, ..
        } => {
            if type_args.is_empty() {
                name.replace('.', "_")
            } else {
                let args: Vec<String> = type_args.iter().map(mangle_type_name).collect();
                format!("{}_{}", name.replace('.', "_"), args.join("_"))
            }
        }
        TypeExprKind::Array(inner) => format!("Array_{}", mangle_type_name(inner)),
        TypeExprKind::Tuple(parts) => {
            let parts: Vec<String> = parts.iter().map(mangle_type_name).collect();
            format!("Tuple_{}", parts.join("_"))
        }
        other => unreachable!("for-block type cannot be mangled: {other:?}"),
    }
}

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
    /// Maps type name -> TypeDef for mock<T> codegen
    type_defs: HashMap<String, TypeDef>,
    /// Locally defined function/const names - these shadow stdlib in pipe resolution
    local_names: HashSet<String>,
    /// Resolved imports from other .fl files, for expanding bare imports.
    resolved_imports: HashMap<String, ResolvedImports>,
    /// Maps original import name -> aliased name for names that conflict with locals.
    import_aliases: HashMap<String, String>,
    /// Whether to emit test blocks (true for `floe test`, false for `floe build`).
    test_mode: bool,
    /// Names used in value positions (expressions). Names only used in type
    /// positions should be emitted as `import type { ... }`.
    value_used_names: HashSet<String>,
    /// Maps (type_name, method_name) → mangled name for for-block functions.
    /// Used to resolve `Entry.toModel` → `Entry__toModel` in call sites.
    for_block_fns: HashMap<(String, String), String>,
    /// Type names that have for-block methods (e.g. "AccentRow" from `for AccentRow { ... }`).
    for_block_type_names: HashSet<String>,
    /// Type names used as runtime constructors (e.g. `User(name: "x")`).
    constructor_used_names: HashSet<String>,
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
            type_defs: HashMap::new(),
            local_names: HashSet::new(),
            resolved_imports: HashMap::new(),
            import_aliases: HashMap::new(),
            test_mode: false,
            value_used_names: HashSet::new(),
            for_block_fns: HashMap::new(),
            for_block_type_names: HashSet::new(),
            constructor_used_names: HashSet::new(),
        }
    }

    /// Enable test mode: test blocks will be emitted instead of stripped.
    pub fn with_test_mode(mut self) -> Self {
        self.test_mode = true;
        self
    }

    /// Create a codegen with resolved import info.
    pub fn with_imports(resolved: &HashMap<String, ResolvedImports>) -> Self {
        let mut codegen = Self::new();
        codegen.resolved_imports = resolved.clone();
        // Pre-register union variant info and type defs from imported types
        for imports in resolved.values() {
            for decl in &imports.type_decls {
                codegen.register_union_variants(decl);
                codegen
                    .type_defs
                    .insert(decl.name.clone(), decl.def.clone());
            }
        }
        codegen
    }

    fn register_union_variants(&mut self, decl: &TypeDecl) {
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
    }

    /// Generate TypeScript from a Floe program.
    pub fn generate(mut self, program: &Program) -> CodegenOutput {
        // Collect names used in value positions for import type detection
        self.value_used_names = collect_value_used_names(program);
        self.constructor_used_names = collect_constructor_names(program);

        // First pass: collect union variant info and local names
        for item in &program.items {
            match &item.kind {
                ItemKind::TypeDecl(decl) => {
                    self.register_union_variants(decl);
                    self.type_defs.insert(decl.name.clone(), decl.def.clone());
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
                    // Register for-block functions from imports
                    if let Some(resolved) = self.resolved_imports.get(&decl.source).cloned() {
                        for block in &resolved.for_blocks {
                            self.register_for_block_fns(block);
                        }
                    }
                }
                ItemKind::ForBlock(block) => {
                    self.register_for_block_fns(block);
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
                            names.push(for_block_fn_name(&block.type_name, &func.name));
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
                    // For npm imports: a specifier is type-only if it's NOT used
                    // in any value position (expression, call, construct).
                    // Names only used as for-block type prefixes (e.g. AccentRow in
                    // AccentRow.toModel) are type-only since the codegen mangles
                    // them away.
                    decl.specifiers
                        .iter()
                        .filter(|spec| {
                            let effective = spec.alias.as_ref().unwrap_or(&spec.name);
                            !self.value_used_names.contains(effective)
                                || self.is_for_block_type_only(effective)
                        })
                        .map(|spec| spec.name.clone())
                        .collect()
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
                        Expr::synthetic(
                            ExprKind::Identifier(format!("{last_temp}.value")),
                            step.expr.span,
                        )
                    } else {
                        Expr::synthetic(ExprKind::Identifier(last_temp.clone()), step.expr.span)
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
                        ConstBinding::Object(fields) => {
                            self.push("{ ");
                            self.emit_object_destructure_fields(fields);
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
            ConstBinding::Object(fields) => {
                self.push("{ ");
                self.emit_object_destructure_fields(fields);
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
            |rt| matches!(&rt.kind, TypeExprKind::Named { name, .. } if name == type_layout::TYPE_UNIT),
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

    fn emit_object_destructure_fields(&mut self, fields: &[ObjectDestructureField]) {
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.push(&f.field);
            if let Some(alias) = &f.alias {
                self.push(": ");
                self.push(alias);
            }
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
                self.emit_object_destructure_fields(fields);
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

    fn register_for_block_fns(&mut self, block: &ForBlock) {
        let type_name = match &block.type_name.kind {
            TypeExprKind::Named { name, .. } => name.clone(),
            _ => return,
        };
        self.for_block_type_names.insert(type_name.clone());
        for func in &block.functions {
            let mangled = for_block_fn_name(&block.type_name, &func.name);
            self.for_block_fns
                .insert((type_name.clone(), func.name.clone()), mangled);
        }
    }

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
        self.push(&for_block_fn_name(for_type, &func.name));
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
            |rt| matches!(&rt.kind, TypeExprKind::Named { name, .. } if name == type_layout::TYPE_UNIT),
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
            if let Some(type_expr) = &spread.type_expr {
                self.emit_type_expr(type_expr);
            } else {
                self.push(&spread.type_name);
            }
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
                if name == type_layout::TYPE_OPTION && type_args.len() == 1 {
                    self.emit_type_expr(&type_args[0]);
                    self.push(" | undefined");
                    return;
                }
                // Settable<T> becomes T | null | undefined
                if name == type_layout::TYPE_SETTABLE && type_args.len() == 1 {
                    self.emit_type_expr(&type_args[0]);
                    self.push(" | null | undefined");
                    return;
                }
                // Result<T, E> becomes { ok: true; value: T } | { ok: false; error: E }
                if name == type_layout::TYPE_RESULT && type_args.len() == 2 {
                    self.push(&format!("{{ {OK_FIELD}: true; {VALUE_FIELD}: "));
                    self.emit_type_expr(&type_args[0]);
                    self.push(&format!(" }} | {{ {OK_FIELD}: false; {ERROR_FIELD}: "));
                    self.emit_type_expr(&type_args[1]);
                    self.push(" }");
                    return;
                }

                // Unit type () becomes void in TypeScript
                if name == type_layout::TYPE_UNIT {
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
            TypeExprKind::TypeOf(name) => {
                self.push(&format!("typeof {name}"));
            }
            TypeExprKind::Intersection(types) => {
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        self.push(" & ");
                    }
                    self.emit_type_expr(ty);
                }
            }
            TypeExprKind::StringLiteral(value) => {
                self.push(&format!("\"{value}\""));
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
                                names.push(for_block_fn_name(&block.type_name, &func.name));
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
                            value_names.push(for_block_fn_name(&block.type_name, &func.name));
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
            ConstBinding::Object(fields) => {
                for f in fields {
                    let name = f.bound_name();
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
        out.push_str(&for_block_fn_name(for_type, &func.name));
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
            type_defs: self.type_defs.clone(),
            local_names: self.local_names.clone(),
            resolved_imports: self.resolved_imports.clone(),
            import_aliases: self.import_aliases.clone(),
            test_mode: self.test_mode,
            value_used_names: self.value_used_names.clone(),
            for_block_fns: self.for_block_fns.clone(),
            for_block_type_names: self.for_block_type_names.clone(),
            constructor_used_names: self.constructor_used_names.clone(),
        }
    }

    /// Returns true if the name is used as a for-block type prefix but NOT
    /// as a runtime value (constructor, call, etc). For-block type prefixes
    /// like `AccentRow` in `AccentRow.toModel` are mangled away by codegen,
    /// but if `AccentRow(...)` also appears, it's still needed at runtime.
    fn is_for_block_type_only(&self, name: &str) -> bool {
        self.for_block_type_names.contains(name) && !self.constructor_used_names.contains(name)
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
    // Replace variadic placeholder ($..) with all args comma-separated
    result = result.replace("$..", &args.join(", "));
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

/// Collect type names used as constructors (e.g. `User(name: "x")`).
fn collect_constructor_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &program.items {
        collect_constructors_from_item(item, &mut names);
    }
    names
}

fn collect_constructors_from_expr(expr: &Expr, names: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Construct {
            type_name,
            args,
            spread,
            ..
        } => {
            names.insert(type_name.clone());
            if let Some(s) = spread {
                collect_constructors_from_expr(s, names);
            }
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        collect_constructors_from_expr(e, names);
                    }
                }
            }
        }
        ExprKind::Block(items) | ExprKind::Collect(items) => {
            for item in items {
                collect_constructors_from_item(item, names);
            }
        }
        ExprKind::Call { callee, args, .. } => {
            collect_constructors_from_expr(callee, names);
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        collect_constructors_from_expr(e, names);
                    }
                }
            }
        }
        ExprKind::Arrow { body, .. } => collect_constructors_from_expr(body, names),
        ExprKind::Pipe { left, right } | ExprKind::Binary { left, right, .. } => {
            collect_constructors_from_expr(left, names);
            collect_constructors_from_expr(right, names);
        }
        ExprKind::Match { subject, arms } => {
            collect_constructors_from_expr(subject, names);
            for arm in arms {
                collect_constructors_from_expr(&arm.body, names);
            }
        }
        ExprKind::Try(e)
        | ExprKind::Unwrap(e)
        | ExprKind::Await(e)
        | ExprKind::Ok(e)
        | ExprKind::Err(e)
        | ExprKind::Some(e)
        | ExprKind::Value(e)
        | ExprKind::Unary { operand: e, .. } => {
            collect_constructors_from_expr(e, names);
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_constructors_from_expr(item, names);
            }
        }
        ExprKind::Jsx(jsx) => collect_constructors_from_jsx(jsx, names),
        _ => {}
    }
}

fn collect_constructors_from_jsx(jsx: &JsxElement, names: &mut HashSet<String>) {
    match &jsx.kind {
        JsxElementKind::Element {
            props, children, ..
        } => {
            for prop in props {
                match prop {
                    JsxProp::Named { value: Some(v), .. } => {
                        collect_constructors_from_expr(v, names)
                    }
                    JsxProp::Spread { expr, .. } => collect_constructors_from_expr(expr, names),
                    _ => {}
                }
            }
            for child in children {
                match child {
                    JsxChild::Expr(e) => collect_constructors_from_expr(e, names),
                    JsxChild::Element(el) => collect_constructors_from_jsx(el, names),
                    _ => {}
                }
            }
        }
        JsxElementKind::Fragment { children } => {
            for child in children {
                match child {
                    JsxChild::Expr(e) => collect_constructors_from_expr(e, names),
                    JsxChild::Element(el) => collect_constructors_from_jsx(el, names),
                    _ => {}
                }
            }
        }
    }
}

fn collect_constructors_from_item(item: &Item, names: &mut HashSet<String>) {
    match &item.kind {
        ItemKind::Const(decl) => collect_constructors_from_expr(&decl.value, names),
        ItemKind::Function(decl) => collect_constructors_from_expr(&decl.body, names),
        ItemKind::ForBlock(block) => {
            for func in &block.functions {
                collect_constructors_from_expr(&func.body, names);
            }
        }
        ItemKind::Expr(expr) => collect_constructors_from_expr(expr, names),
        _ => {}
    }
}

/// Collect all names used in value positions (expressions, not type annotations).
/// Used to detect type-only imports for `import type { ... }` codegen.
fn collect_value_used_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &program.items {
        collect_value_names_from_item(item, &mut names);
    }
    names
}

fn collect_value_names_from_expr(expr: &Expr, names: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Identifier(name) => {
            names.insert(name.clone());
        }
        ExprKind::Construct {
            type_name,
            args,
            spread,
            ..
        } => {
            names.insert(type_name.clone());
            if let Some(s) = spread {
                collect_value_names_from_expr(s, names);
            }
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        collect_value_names_from_expr(e, names);
                    }
                }
            }
        }
        ExprKind::Call { callee, args, .. } => {
            collect_value_names_from_expr(callee, names);
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        collect_value_names_from_expr(e, names);
                    }
                }
            }
        }
        ExprKind::Block(items) | ExprKind::Collect(items) => {
            for item in items {
                collect_value_names_from_item(item, names);
            }
        }
        ExprKind::Arrow { body, .. } => collect_value_names_from_expr(body, names),
        ExprKind::Member { object, .. } => collect_value_names_from_expr(object, names),
        ExprKind::Pipe { left, right } | ExprKind::Binary { left, right, .. } => {
            collect_value_names_from_expr(left, names);
            collect_value_names_from_expr(right, names);
        }
        ExprKind::Unary { operand, .. } => collect_value_names_from_expr(operand, names),
        ExprKind::Try(e) | ExprKind::Unwrap(e) | ExprKind::Await(e) => {
            collect_value_names_from_expr(e, names);
        }
        ExprKind::Ok(e) | ExprKind::Err(e) | ExprKind::Some(e) | ExprKind::Value(e) => {
            collect_value_names_from_expr(e, names);
        }
        ExprKind::Match { subject, arms } => {
            collect_value_names_from_expr(subject, names);
            for arm in arms {
                collect_value_names_from_expr(&arm.body, names);
                if let Some(guard) = &arm.guard {
                    collect_value_names_from_expr(guard, names);
                }
            }
        }
        ExprKind::Index { object, index } => {
            collect_value_names_from_expr(object, names);
            collect_value_names_from_expr(index, names);
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_value_names_from_expr(item, names);
            }
        }
        ExprKind::TemplateLiteral(parts) => {
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    collect_value_names_from_expr(e, names);
                }
            }
        }
        ExprKind::Jsx(jsx) => collect_value_names_from_jsx(jsx, names),
        _ => {}
    }
}

fn collect_value_names_from_jsx(jsx: &JsxElement, names: &mut HashSet<String>) {
    match &jsx.kind {
        JsxElementKind::Element {
            name,
            props,
            children,
            ..
        } => {
            // Uppercase component names are value references (e.g. <QueryClientProvider>)
            if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                names.insert(name.clone());
            }
            for prop in props {
                match prop {
                    JsxProp::Named { value, .. } => {
                        if let Some(value) = value {
                            collect_value_names_from_expr(value, names);
                        }
                    }
                    JsxProp::Spread { expr, .. } => {
                        collect_value_names_from_expr(expr, names);
                    }
                }
            }
            for child in children {
                match child {
                    JsxChild::Expr(e) => collect_value_names_from_expr(e, names),
                    JsxChild::Element(el) => collect_value_names_from_jsx(el, names),
                    JsxChild::Text(_) => {}
                }
            }
        }
        JsxElementKind::Fragment { children } => {
            for child in children {
                match child {
                    JsxChild::Expr(e) => collect_value_names_from_expr(e, names),
                    JsxChild::Element(el) => collect_value_names_from_jsx(el, names),
                    JsxChild::Text(_) => {}
                }
            }
        }
    }
}

fn collect_value_names_from_item(item: &Item, names: &mut HashSet<String>) {
    match &item.kind {
        ItemKind::Const(decl) => collect_value_names_from_expr(&decl.value, names),
        ItemKind::Function(decl) => collect_value_names_from_expr(&decl.body, names),
        ItemKind::ForBlock(block) => {
            for func in &block.functions {
                collect_value_names_from_expr(&func.body, names);
            }
        }
        ItemKind::Expr(expr) => collect_value_names_from_expr(expr, names),
        _ => {}
    }
}
