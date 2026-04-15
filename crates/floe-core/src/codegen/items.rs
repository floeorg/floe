use super::*;

impl Codegen {
    // ── Items ────────────────────────────────────────────────────

    pub(super) fn emit_item(&mut self, item: &TypedItem) {
        match &item.kind {
            ItemKind::Import(decl) => self.emit_import(decl),
            ItemKind::ReExport(decl) => self.emit_reexport(decl),
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
        if decl.specifiers.is_empty()
            && decl.for_specifiers.is_empty()
            && decl.default_import.is_none()
        {
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
            // Default import: `import X from "..."` or `import X, { a, b } from "..."`
            if let Some(ref default_name) = decl.default_import {
                self.push(&format!("import {default_name}"));
                if !decl.specifiers.is_empty() {
                    self.push(", { ");
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
                    self.push(" }");
                }
                self.push(&format!(" from \"{}\";", decl.source));
            } else {
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
    }

    // ── Re-export ────────────────────────────────────────────────

    fn emit_reexport(&mut self, decl: &ReExportDecl) {
        self.emit_indent();
        self.push("export { ");
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

    // ── Const ────────────────────────────────────────────────────

    fn emit_const(&mut self, decl: &TypedConstDecl) {
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
                        TypedExpr::synthetic_typed(
                            ExprKind::Identifier(format!("{last_temp}.value")),
                            step.expr.span,
                        )
                    } else {
                        TypedExpr::synthetic_typed(
                            ExprKind::Identifier(last_temp.clone()),
                            step.expr.span,
                        )
                    };
                    let mut sub = self.sub_codegen();
                    sub.emit_pipe(&left_expr, &step.expr);
                    if sub.needs_deep_equal {
                        self.needs_deep_equal = true;
                    }
                    if sub.has_jsx {
                        self.has_jsx = true;
                    }
                    sub.output
                } else {
                    let mut sub = self.sub_codegen();
                    sub.emit_expr(&step.expr);
                    if sub.needs_deep_equal {
                        self.needs_deep_equal = true;
                    }
                    if sub.has_jsx {
                        self.has_jsx = true;
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

    fn emit_function(&mut self, decl: &TypedFunctionDecl) {
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
            for (i, tp) in decl.type_params.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(&tp.name);
                if !tp.bounds.is_empty() {
                    self.push(" extends ");
                    self.push(&tp.bounds.join(" & "));
                }
            }
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
            // For `async fn f() -> T`, the source has T but TS requires Promise<T>
            // on async functions. Wrap the annotation in Promise<>.
            // If the annotation is already Promise<T> (or `uses_await` detected it),
            // emit as-is.
            let needs_promise_wrap = decl.async_fn
                && !matches!(&ret.kind, TypeExprKind::Named { name, type_args, .. } if name == "Promise" && !type_args.is_empty());
            if needs_promise_wrap {
                self.push("Promise<");
                self.emit_type_expr(ret);
                self.push(">");
            } else {
                self.emit_type_expr(ret);
            }
        }

        // Track type param bounds so pipe dispatch can use method calls
        // for generic-bounded values (e.g. `repo |> create(input)` → `repo.create(input)`)
        for tp in &decl.type_params {
            if !tp.bounds.is_empty() {
                self.current_type_param_bounds
                    .insert(tp.name.clone(), tp.bounds.clone());
            }
        }

        self.push(" ");
        if is_unit_return {
            self.emit_block_expr(&decl.body);
        } else {
            self.emit_block_expr_with_return(&decl.body);
        }

        self.current_type_param_bounds.clear();
    }

    pub(super) fn emit_object_destructure_fields(&mut self, fields: &[ObjectDestructureField]) {
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

    pub(super) fn emit_params(&mut self, params: &[TypedParam]) {
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.emit_param(param);
        }
    }

    pub(super) fn emit_param(&mut self, param: &TypedParam) {
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

    pub(super) fn register_for_block_fns<T: std::fmt::Debug>(&mut self, block: &ForBlock<T>) {
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

    fn emit_for_block(&mut self, block: &TypedForBlock) {
        for (i, func) in block.functions.iter().enumerate() {
            if i > 0 {
                self.newline();
            }
            self.emit_for_block_function(func, &block.type_name);
        }
        // For trait impls used as generic bounds, emit a factory function
        // so instances satisfy the TypeScript interface
        if let Some(trait_name) = &block.trait_name
            && self.traits_needing_interface.contains(trait_name.as_str())
        {
            self.newline();
            self.emit_trait_impl_factory(block);
        }
    }

    /// Emit a factory function for a `for Type: Trait` block.
    ///
    /// The factory wraps the standalone for-block functions as method properties
    /// on the returned object, so that `Type` instances satisfy the TypeScript
    /// interface emitted for the trait.
    ///
    /// Example: `for DrizzleSnippetRepository: SnippetRepository { fn create(...) }` emits:
    /// ```typescript
    /// function DrizzleSnippetRepository__make(data: { client: Database }): DrizzleSnippetRepository {
    ///   return { ...data, create: (input) => DrizzleSnippetRepository__create(data, input) };
    /// }
    /// ```
    fn emit_trait_impl_factory(&mut self, block: &TypedForBlock) {
        let type_name = match &block.type_name.kind {
            TypeExprKind::Named { name, .. } => name.clone(),
            _ => return,
        };

        let factory_name = format!("{type_name}__make");
        self.emit_indent();
        self.push("function ");
        self.push(&factory_name);
        self.push("(__data: ");
        self.emit_type_expr(&block.type_name);
        self.push("): ");
        self.emit_type_expr(&block.type_name);
        self.push(" {\n");
        self.indent += 1;
        self.emit_indent();
        self.push("return {\n");
        self.indent += 1;
        self.emit_indent();
        self.push("...__data,\n");

        for func in &block.functions {
            // Skip self param — methods only take the non-self params
            let non_self_params: Vec<&TypedParam> =
                func.params.iter().filter(|p| p.name != "self").collect();
            let mangled = for_block_fn_name(&block.type_name, &func.name);

            self.emit_indent();
            self.push(&func.name);
            self.push(": (");
            for (i, param) in non_self_params.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(&param.name);
                if let Some(ta) = &param.type_ann {
                    self.push(": ");
                    self.emit_type_expr(ta);
                }
            }
            self.push(") => ");
            self.push(&mangled);
            self.push("(__data");
            for param in &non_self_params {
                self.push(", ");
                self.push(&param.name);
            }
            self.push("),\n");
        }

        self.indent -= 1;
        self.emit_indent();
        self.push("};\n");
        self.indent -= 1;
        self.emit_indent();
        self.push("}");
    }

    /// Emit TypeScript interfaces for all traits used as generic bounds.
    /// Returns the emitted string (empty if none needed).
    pub(super) fn emit_trait_interfaces(&mut self) -> String {
        if self.traits_needing_interface.is_empty() {
            return String::new();
        }

        let trait_names: Vec<String> = self.traits_needing_interface.iter().cloned().collect();
        let mut out = String::new();

        for trait_name in &trait_names {
            let Some(decl) = self.trait_decls.get(trait_name).cloned() else {
                continue;
            };

            out.push_str("interface ");
            out.push_str(trait_name);
            out.push_str(" {\n");

            for method in &decl.methods {
                // Skip self — TypeScript interfaces have implicit `this`
                let non_self_params: Vec<&TypedParam> =
                    method.params.iter().filter(|p| p.name != "self").collect();

                out.push_str("  ");
                out.push_str(&method.name);
                out.push('(');
                for (i, param) in non_self_params.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&param.name);
                    if let Some(ta) = &param.type_ann {
                        out.push_str(": ");
                        let mut cg = self.sub_codegen();
                        cg.emit_type_expr(ta);
                        out.push_str(&cg.output);
                    }
                }
                out.push(')');
                if let Some(rt) = &method.return_type {
                    out.push_str(": ");
                    let mut cg = self.sub_codegen();
                    cg.emit_type_expr(rt);
                    out.push_str(&cg.output);
                }
                out.push_str(";\n");
            }

            out.push('}');
        }

        out
    }

    fn emit_for_block_function(&mut self, func: &TypedFunctionDecl, for_type: &TypedTypeExpr) {
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
            let needs_promise_wrap = func.async_fn
                && !matches!(&ret.kind, TypeExprKind::Named { name, type_args, .. } if name == "Promise" && !type_args.is_empty());
            if needs_promise_wrap {
                self.push("Promise<");
                self.emit_type_expr(ret);
                self.push(">");
            } else {
                self.emit_type_expr(ret);
            }
        }

        self.push(" ");
        if is_unit_return {
            self.emit_block_expr(&func.body);
        } else {
            self.emit_block_expr_with_return(&func.body);
        }
    }

    // ── Test Blocks ──────────────────────────────────────────────

    fn emit_test_block(&mut self, block: &TypedTestBlock) {
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

    fn emit_type_decl(&mut self, decl: &TypedTypeDecl) {
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

    fn emit_derived_display(&mut self, type_name: &str, fields: &[&TypedRecordField]) {
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

    pub(super) fn emit_record_type_entries(&mut self, entries: &[TypedRecordEntry]) {
        let spreads: Vec<&TypedRecordSpread> =
            entries.iter().filter_map(|e| e.as_spread()).collect();
        let fields: Vec<&TypedRecordField> = entries.iter().filter_map(|e| e.as_field()).collect();

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

    fn emit_record_type_fields(&mut self, fields: &[&TypedRecordField]) {
        self.push("{ ");
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.push("; ");
            }
            self.push(&field.name);
            if field.default.is_some() {
                self.push("?");
            }
            self.push(": ");
            self.emit_type_expr(&field.type_ann);
        }
        self.push(" }");
    }

    pub(super) fn emit_record_type(&mut self, fields: &[TypedRecordField]) {
        let refs: Vec<&TypedRecordField> = fields.iter().collect();
        self.emit_record_type_fields(&refs);
    }

    pub(super) fn emit_union_type(&mut self, variants: &[TypedVariant]) {
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
                for (fi, field) in variant.fields.iter().enumerate() {
                    self.push("; ");
                    if let Some(name) = &field.name {
                        self.push(name);
                    } else {
                        self.push(&type_layout::positional_field_name(
                            fi,
                            variant.fields.len(),
                        ));
                    }
                    self.push(": ");
                    self.emit_type_expr(&field.type_ann);
                }
                self.push(" }");
            }
        }
    }

    pub(super) fn emit_string_literal_union_type(&mut self, variants: &[String]) {
        for (i, variant) in variants.iter().enumerate() {
            if i > 0 {
                self.push(" | ");
            }
            self.push(&format!("\"{}\"", escape_string(variant)));
        }
    }

    /// Resolve `for Type` import specifiers to concrete function names.
    pub(super) fn resolve_for_import_names(&self, decl: &ImportDecl) -> Vec<String> {
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
}
