use crate::parser::ast::*;
use crate::pretty::{self, Document};
use crate::type_layout;
use crate::type_layout::TAG_FIELD;

use super::super::{escape_string, for_block_fn_name};
use super::generator::TypeScriptGenerator;

impl<'a> TypeScriptGenerator<'a> {
    // ── Items ────────────────────────────────────────────────────

    pub(super) fn emit_item(&mut self, item: &TypedItem) -> Document {
        match &item.kind {
            ItemKind::Import(decl) => self.emit_import(decl),
            ItemKind::ReExport(decl) => self.emit_reexport(decl),
            ItemKind::Const(decl) => self.emit_const(decl),
            ItemKind::Function(decl) => self.emit_function(decl),
            ItemKind::TypeDecl(decl) => self.emit_type_decl(decl),
            ItemKind::ForBlock(block) => self.emit_for_block(block),
            ItemKind::TraitDecl(_) => pretty::nil(),
            ItemKind::TestBlock(block) => self.emit_test_block(block),
            ItemKind::Expr(expr) => pretty::concat([self.emit_expr(expr), pretty::str(";")]),
        }
    }

    // ── Import ───────────────────────────────────────────────────

    fn emit_import(&mut self, decl: &ImportDecl) -> Document {
        if decl.specifiers.is_empty()
            && decl.for_specifiers.is_empty()
            && decl.default_import.is_none()
        {
            if let Some(resolved) = self.ctx.resolved_imports.get(&decl.source) {
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
                    return pretty::str(format!("import \"{}\";", decl.source));
                }
                let specifiers: Vec<String> = names
                    .iter()
                    .map(|name| {
                        let alias = format!("__{name}");
                        self.import_aliases.insert(name.clone(), alias.clone());
                        format!("{name} as {alias}")
                    })
                    .collect();
                return pretty::str(format!(
                    "import {{ {} }} from \"{}\";",
                    specifiers.join(", "),
                    decl.source
                ));
            }
            return pretty::str(format!("import \"{}\";", decl.source));
        }

        let type_only_names: std::collections::HashSet<String> =
            if let Some(resolved) = self.ctx.resolved_imports.get(&decl.source) {
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
                decl.specifiers
                    .iter()
                    .filter(|spec| {
                        let effective = spec.alias.as_ref().unwrap_or(&spec.name);
                        !self.ctx.value_used_names.contains(effective)
                            || self.ctx.is_for_block_type_only(effective)
                    })
                    .map(|spec| spec.name.clone())
                    .collect()
            };

        if let Some(ref default_name) = decl.default_import {
            let mut s = format!("import {default_name}");
            if !decl.specifiers.is_empty() {
                s.push_str(", { ");
                let mut first = true;
                for spec in &decl.specifiers {
                    if !first {
                        s.push_str(", ");
                    }
                    first = false;
                    if type_only_names.contains(&spec.name) {
                        s.push_str("type ");
                    }
                    s.push_str(&spec.name);
                    if let Some(alias) = &spec.alias {
                        s.push_str(" as ");
                        s.push_str(alias);
                    }
                }
                s.push_str(" }");
            }
            s.push_str(&format!(" from \"{}\";", decl.source));
            return pretty::str(s);
        }

        let mut s = String::from("import { ");
        let mut first = true;
        for spec in &decl.specifiers {
            if !first {
                s.push_str(", ");
            }
            first = false;
            if type_only_names.contains(&spec.name) {
                s.push_str("type ");
            }
            s.push_str(&spec.name);
            if let Some(alias) = &spec.alias {
                s.push_str(" as ");
                s.push_str(alias);
            }
        }
        let for_func_names = self.resolve_for_import_names(decl);
        for name in &for_func_names {
            if !first {
                s.push_str(", ");
            }
            first = false;
            s.push_str(name);
        }
        s.push_str(&format!(" }} from \"{}\";", decl.source));
        pretty::str(s)
    }

    // ── Re-export ────────────────────────────────────────────────

    fn emit_reexport(&self, decl: &ReExportDecl) -> Document {
        let mut s = String::from("export { ");
        for (i, spec) in decl.specifiers.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&spec.name);
            if let Some(alias) = &spec.alias {
                s.push_str(" as ");
                s.push_str(alias);
            }
        }
        s.push_str(&format!(" }} from \"{}\";", decl.source));
        pretty::str(s)
    }

    // ── Const ────────────────────────────────────────────────────

    fn emit_const(&mut self, decl: &TypedConstDecl) -> Document {
        if Self::expr_has_unwrap(&decl.value) {
            return self.emit_const_unwrap(decl);
        }

        let mut docs = Vec::new();
        if decl.exported {
            docs.push(pretty::str("export "));
        }
        docs.push(pretty::str("const "));
        docs.push(self.emit_binding(&decl.binding));
        if let Some(type_ann) = &decl.type_ann {
            docs.push(pretty::str(": "));
            docs.push(self.emit_type_expr(type_ann));
        }
        docs.push(pretty::str(" = "));
        docs.push(self.emit_expr(&decl.value));
        docs.push(pretty::str(";"));
        pretty::concat(docs)
    }

    fn emit_const_unwrap(&mut self, decl: &TypedConstDecl) -> Document {
        let steps = Self::flatten_pipe_unwrap_chain(&decl.value);
        let mut docs = Vec::new();
        let mut last_temp = String::new();
        let mut last_had_unwrap = false;

        for (i, step) in steps.iter().enumerate() {
            let temp = format!("_r{}", self.unwrap_counter);
            self.unwrap_counter += 1;

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
                let pipe_doc = self.emit_pipe(&left_expr, &step.expr);
                Self::doc_to_string(&pipe_doc)
            } else {
                self.emit_expr_string(&step.expr)
            };

            let needs_await = step.is_await || step_code.starts_with("(async ");

            if i > 0 {
                docs.push(pretty::line());
            }
            if needs_await {
                docs.push(pretty::str(format!("const {temp} = await {step_code};")));
            } else {
                docs.push(pretty::str(format!("const {temp} = {step_code};")));
            }

            if step.unwrap {
                docs.push(pretty::line());
                docs.push(pretty::str(format!("if (!{temp}.ok) return {temp};")));
                last_had_unwrap = true;
            } else {
                last_had_unwrap = false;
            }
            last_temp = temp;

            if i == steps.len() - 1 {
                let value_expr = if last_had_unwrap {
                    format!("{last_temp}.value")
                } else {
                    last_temp.clone()
                };

                docs.push(pretty::line());
                let mut binding_docs = Vec::new();
                if decl.exported {
                    binding_docs.push(pretty::str("export "));
                }
                binding_docs.push(pretty::str("const "));
                binding_docs.push(self.emit_binding(&decl.binding));
                binding_docs.push(pretty::str(format!(" = {value_expr};")));
                docs.push(pretty::concat(binding_docs));
            }
        }
        pretty::concat(docs)
    }

    fn emit_binding(&mut self, binding: &ConstBinding) -> Document {
        match binding {
            ConstBinding::Name(name) => pretty::str(name),
            ConstBinding::Array(names) | ConstBinding::Tuple(names) => {
                pretty::str(format!("[{}]", names.join(", ")))
            }
            ConstBinding::Object(fields) => {
                let mut s = String::from("{ ");
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&f.field);
                    if let Some(alias) = &f.alias {
                        s.push_str(": ");
                        s.push_str(alias);
                    }
                }
                s.push_str(" }");
                pretty::str(s)
            }
        }
    }

    // ── Function ─────────────────────────────────────────────────

    fn emit_function(&mut self, decl: &TypedFunctionDecl) -> Document {
        // Derived function binding: fn name = expr → const name = expr;
        if decl.params.is_empty()
            && decl.return_type.is_none()
            && !matches!(decl.body.kind, ExprKind::Block(_))
        {
            let mut docs = Vec::new();
            if decl.exported {
                docs.push(pretty::str("export "));
            }
            docs.push(pretty::str("const "));
            docs.push(pretty::str(&decl.name));
            docs.push(pretty::str(" = "));
            docs.push(self.emit_expr(&decl.body));
            docs.push(pretty::str(";"));
            return pretty::concat(docs);
        }

        let mut docs = Vec::new();
        if decl.exported {
            docs.push(pretty::str("export "));
        }
        if decl.async_fn {
            docs.push(pretty::str("async "));
        }
        docs.push(pretty::str("function "));
        docs.push(pretty::str(&decl.name));
        if !decl.type_params.is_empty() {
            docs.push(self.emit_type_params(&decl.type_params));
        }
        docs.push(pretty::str("("));
        docs.push(self.emit_params(&decl.params));
        docs.push(pretty::str(")"));

        let is_unit_return = decl.return_type.as_ref().is_some_and(
            |rt| matches!(&rt.kind, TypeExprKind::Named { name, .. } if name == type_layout::TYPE_UNIT),
        );

        if let Some(ret) = &decl.return_type {
            docs.push(pretty::str(": "));
            let needs_promise_wrap = decl.async_fn
                && !matches!(&ret.kind, TypeExprKind::Named { name, type_args, .. } if name == "Promise" && !type_args.is_empty());
            if needs_promise_wrap {
                docs.push(pretty::str("Promise<"));
                docs.push(self.emit_type_expr(ret));
                docs.push(pretty::str(">"));
            } else {
                docs.push(self.emit_type_expr(ret));
            }
        }

        for tp in &decl.type_params {
            if !tp.bounds.is_empty() {
                self.current_type_param_bounds
                    .insert(tp.name.clone(), tp.bounds.clone());
            }
        }

        docs.push(pretty::str(" "));
        if is_unit_return {
            docs.push(self.emit_block_expr(&decl.body));
        } else {
            docs.push(self.emit_block_expr_with_return(&decl.body));
        }

        self.current_type_param_bounds.clear();
        pretty::concat(docs)
    }

    fn emit_type_params(&self, type_params: &[TypeParam]) -> Document {
        let mut s = String::from("<");
        for (i, tp) in type_params.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&tp.name);
            if !tp.bounds.is_empty() {
                s.push_str(" extends ");
                s.push_str(&tp.bounds.join(" & "));
            }
        }
        s.push('>');
        pretty::str(s)
    }

    pub(super) fn emit_object_destructure_fields(
        &self,
        fields: &[ObjectDestructureField],
    ) -> String {
        let mut s = String::new();
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&f.field);
            if let Some(alias) = &f.alias {
                s.push_str(": ");
                s.push_str(alias);
            }
        }
        s
    }

    pub(super) fn emit_params(&mut self, params: &[TypedParam]) -> Document {
        let mut docs = Vec::new();
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                docs.push(pretty::str(", "));
            }
            docs.push(self.emit_param(param));
        }
        pretty::concat(docs)
    }

    pub(super) fn emit_param(&mut self, param: &TypedParam) -> Document {
        let mut docs = Vec::new();
        match &param.destructure {
            Some(ParamDestructure::Object(fields)) => {
                docs.push(pretty::str(format!(
                    "{{ {} }}",
                    self.emit_object_destructure_fields(fields)
                )));
            }
            Some(ParamDestructure::Array(fields)) => {
                docs.push(pretty::str(format!("[{}]", fields.join(", "))));
            }
            None => {
                docs.push(pretty::str(&param.name));
            }
        }
        if let Some(type_ann) = &param.type_ann {
            docs.push(pretty::str(": "));
            docs.push(self.emit_type_expr(type_ann));
        }
        if let Some(default) = &param.default {
            docs.push(pretty::str(" = "));
            docs.push(self.emit_expr(default));
        }
        pretty::concat(docs)
    }

    // ── For Blocks ────────────────────────────────────────────────

    fn emit_for_block(&mut self, block: &TypedForBlock) -> Document {
        let mut docs = Vec::new();
        for (i, func) in block.functions.iter().enumerate() {
            if i > 0 {
                docs.push(pretty::str("\n"));
            }
            docs.push(self.emit_for_block_function(func, &block.type_name));
        }
        if let Some(trait_name) = &block.trait_name
            && self
                .ctx
                .traits_needing_interface
                .contains(trait_name.as_str())
        {
            docs.push(pretty::str("\n"));
            docs.push(self.emit_trait_impl_factory(block));
        }
        pretty::concat(docs)
    }

    fn emit_trait_impl_factory(&mut self, block: &TypedForBlock) -> Document {
        let type_name = match &block.type_name.kind {
            TypeExprKind::Named { name, .. } => name.clone(),
            _ => return pretty::nil(),
        };

        let factory_name = format!("{type_name}__make");
        let mut docs = vec![
            pretty::str("function "),
            pretty::str(&factory_name),
            pretty::str("(__data: "),
            self.emit_type_expr(&block.type_name),
            pretty::str("): "),
            self.emit_type_expr(&block.type_name),
            pretty::str(" {"),
        ];

        let mut inner = vec![pretty::line(), pretty::str("return {")];

        let mut return_inner = Vec::new();
        return_inner.push(pretty::line());
        return_inner.push(pretty::str("...__data,"));

        for func in &block.functions {
            let non_self_params: Vec<&TypedParam> =
                func.params.iter().filter(|p| p.name != "self").collect();
            let mangled = for_block_fn_name(&block.type_name, &func.name);

            return_inner.push(pretty::line());
            let mut param_parts = Vec::new();
            for (i, param) in non_self_params.iter().enumerate() {
                if i > 0 {
                    param_parts.push(", ".to_string());
                }
                let mut p = param.name.clone();
                if let Some(ta) = &param.type_ann {
                    p.push_str(": ");
                    let type_doc = self.emit_type_expr(ta);
                    p.push_str(&Self::doc_to_string(&type_doc));
                }
                param_parts.push(p);
            }
            let params_str = param_parts.join("");

            let call_args: Vec<String> = non_self_params.iter().map(|p| p.name.clone()).collect();
            let call_args_str = if call_args.is_empty() {
                String::new()
            } else {
                format!(", {}", call_args.join(", "))
            };

            return_inner.push(pretty::str(format!(
                "{}: ({params_str}) => {mangled}(__data{call_args_str}),",
                func.name
            )));
        }

        inner.push(pretty::nest(2, pretty::concat(return_inner)));
        inner.push(pretty::line());
        inner.push(pretty::str("};"));

        docs.push(pretty::nest(2, pretty::concat(inner)));
        docs.push(pretty::line());
        docs.push(pretty::str("}"));
        pretty::concat(docs)
    }

    pub(super) fn emit_trait_interfaces(&mut self) -> Document {
        if self.ctx.traits_needing_interface.is_empty() {
            return pretty::nil();
        }

        let trait_names: Vec<String> = self.ctx.traits_needing_interface.iter().cloned().collect();
        let mut docs = Vec::new();

        for trait_name in &trait_names {
            let Some(decl) = self.ctx.trait_decls.get(trait_name).cloned() else {
                continue;
            };

            let mut s = format!("interface {trait_name} {{\n");

            for method in &decl.methods {
                let non_self_params: Vec<&TypedParam> =
                    method.params.iter().filter(|p| p.name != "self").collect();

                s.push_str("  ");
                s.push_str(&method.name);
                s.push('(');
                for (i, param) in non_self_params.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&param.name);
                    if let Some(ta) = &param.type_ann {
                        s.push_str(": ");
                        let type_doc = self.emit_type_expr(ta);
                        s.push_str(&Self::doc_to_string(&type_doc));
                    }
                }
                s.push(')');
                if let Some(rt) = &method.return_type {
                    s.push_str(": ");
                    let type_doc = self.emit_type_expr(rt);
                    s.push_str(&Self::doc_to_string(&type_doc));
                }
                s.push_str(";\n");
            }

            s.push('}');
            docs.push(pretty::str(s));
        }

        pretty::concat(docs)
    }

    fn emit_for_block_function(
        &mut self,
        func: &TypedFunctionDecl,
        for_type: &TypedTypeExpr,
    ) -> Document {
        let mut docs = Vec::new();
        if func.exported {
            docs.push(pretty::str("export "));
        }
        if func.async_fn {
            docs.push(pretty::str("async "));
        }
        docs.push(pretty::str("function "));
        docs.push(pretty::str(for_block_fn_name(for_type, &func.name)));
        docs.push(pretty::str("("));

        let mut param_docs = Vec::new();
        for (i, param) in func.params.iter().enumerate() {
            if i > 0 {
                param_docs.push(pretty::str(", "));
            }
            param_docs.push(pretty::str(&param.name));
            if param.name == "self" {
                param_docs.push(pretty::str(": "));
                param_docs.push(self.emit_type_expr(for_type));
            } else if let Some(type_ann) = &param.type_ann {
                param_docs.push(pretty::str(": "));
                param_docs.push(self.emit_type_expr(type_ann));
            }
            if let Some(default) = &param.default {
                param_docs.push(pretty::str(" = "));
                param_docs.push(self.emit_expr(default));
            }
        }
        docs.push(pretty::concat(param_docs));
        docs.push(pretty::str(")"));

        let is_unit_return = func.return_type.as_ref().is_some_and(
            |rt| matches!(&rt.kind, TypeExprKind::Named { name, .. } if name == type_layout::TYPE_UNIT),
        );

        if let Some(ret) = &func.return_type {
            docs.push(pretty::str(": "));
            let needs_promise_wrap = func.async_fn
                && !matches!(&ret.kind, TypeExprKind::Named { name, type_args, .. } if name == "Promise" && !type_args.is_empty());
            if needs_promise_wrap {
                docs.push(pretty::str("Promise<"));
                docs.push(self.emit_type_expr(ret));
                docs.push(pretty::str(">"));
            } else {
                docs.push(self.emit_type_expr(ret));
            }
        }

        docs.push(pretty::str(" "));
        if is_unit_return {
            docs.push(self.emit_block_expr(&func.body));
        } else {
            docs.push(self.emit_block_expr_with_return(&func.body));
        }

        pretty::concat(docs)
    }

    // ── Test Blocks ──────────────────────────────────────────────

    fn emit_test_block(&mut self, block: &TypedTestBlock) -> Document {
        if !self.ctx.test_mode {
            return pretty::nil();
        }

        let mut inner = Vec::new();
        inner.push(pretty::line());
        inner.push(pretty::str(format!(
            "const __testName = \"{}\";",
            escape_string(&block.name)
        )));
        inner.push(pretty::line());
        inner.push(pretty::str("let __passed = 0;"));
        inner.push(pretty::line());
        inner.push(pretty::str("let __failed = 0;"));

        for stmt in &block.body {
            match stmt {
                TestStatement::Assert(expr, _) => {
                    let expr_doc = self.emit_expr(expr);
                    let expr_str = Self::doc_to_string(&expr_doc);
                    let escaped = escape_string(&expr_str);
                    inner.push(pretty::line());
                    inner.push(pretty::str(format!(
                        "try {{ if (!({expr_str})) {{ __failed++; console.error(`  FAIL: {escaped}`); }} else {{ __passed++; }} }} catch (e) {{ __failed++; console.error(`  FAIL: {escaped}`, e); }}"
                    )));
                }
                TestStatement::Expr(expr) => {
                    inner.push(pretty::line());
                    let expr_doc = self.emit_expr(expr);
                    inner.push(pretty::concat([expr_doc, pretty::str(";")]));
                }
            }
        }

        inner.push(pretty::line());
        inner.push(pretty::str("if (__failed > 0) { console.error(`FAIL ${__testName}: ${__passed} passed, ${__failed} failed`); process.exitCode = 1; }"));
        inner.push(pretty::line());
        inner.push(pretty::str(
            "else { console.log(`PASS ${__testName}: ${__passed} passed`); }",
        ));

        pretty::concat([
            pretty::str(format!("// test: {}", escape_string(&block.name))),
            pretty::str("\n"),
            pretty::str("(function() {"),
            pretty::nest(2, pretty::concat(inner)),
            pretty::line(),
            pretty::str("})();"),
        ])
    }

    // ── Type Declarations ────────────────────────────────────────

    fn emit_type_decl(&mut self, decl: &TypedTypeDecl) -> Document {
        let mut docs = Vec::new();
        if decl.exported {
            docs.push(pretty::str("export "));
        }
        docs.push(pretty::str("type "));
        docs.push(pretty::str(&decl.name));

        if !decl.type_params.is_empty() {
            let params = decl.type_params.join(", ");
            docs.push(pretty::str(format!("<{params}>")));
        }

        docs.push(pretty::str(" = "));

        match &decl.def {
            TypeDef::Record(entries) => docs.push(self.emit_record_type_entries(entries)),
            TypeDef::Union(variants) => docs.push(self.emit_union_type(variants)),
            TypeDef::StringLiteralUnion(variants) => {
                docs.push(self.emit_string_literal_union_type(variants))
            }
            TypeDef::Alias(type_expr) => docs.push(self.emit_type_expr(type_expr)),
        }

        docs.push(pretty::str(";"));

        if !decl.deriving.is_empty()
            && let TypeDef::Record(_) = &decl.def
        {
            let fields = decl.def.record_fields();
            for trait_name in &decl.deriving {
                if trait_name.as_str() == "Display" {
                    docs.push(pretty::str("\n\n"));
                    docs.push(self.emit_derived_display(&decl.name, &fields));
                }
            }
        }

        pretty::concat(docs)
    }

    fn emit_derived_display(&self, type_name: &str, fields: &[&TypedRecordField]) -> Document {
        let mut field_parts = Vec::new();
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                field_parts.push(", ".to_string());
            }
            field_parts.push(format!("{}: ${{self.{}}}", field.name, field.name));
        }
        let fields_str = field_parts.join("");

        pretty::concat([
            pretty::str(format!("function display(self: {type_name}): string {{")),
            pretty::nest(
                2,
                pretty::concat([
                    pretty::line(),
                    pretty::str(format!("return `{type_name}({fields_str})`;")),
                ]),
            ),
            pretty::line(),
            pretty::str("}"),
        ])
    }

    pub(super) fn emit_record_type_entries(&mut self, entries: &[TypedRecordEntry]) -> Document {
        let spreads: Vec<&TypedRecordSpread> =
            entries.iter().filter_map(|e| e.as_spread()).collect();
        let fields: Vec<&TypedRecordField> = entries.iter().filter_map(|e| e.as_field()).collect();

        let mut docs = Vec::new();
        for (i, spread) in spreads.iter().enumerate() {
            if let Some(type_expr) = &spread.type_expr {
                docs.push(self.emit_type_expr(type_expr));
            } else {
                docs.push(pretty::str(&spread.type_name));
            }
            if !fields.is_empty() || i < spreads.len() - 1 {
                docs.push(pretty::str(" & "));
            }
        }

        if !fields.is_empty() || spreads.is_empty() {
            docs.push(self.emit_record_type_fields(&fields));
        }

        pretty::concat(docs)
    }

    fn emit_record_type_fields(&mut self, fields: &[&TypedRecordField]) -> Document {
        let mut docs = vec![pretty::str("{ ")];
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                docs.push(pretty::str("; "));
            }
            docs.push(pretty::str(&field.name));
            if field.default.is_some() {
                docs.push(pretty::str("?"));
            }
            docs.push(pretty::str(": "));
            docs.push(self.emit_type_expr(&field.type_ann));
        }
        docs.push(pretty::str(" }"));
        pretty::concat(docs)
    }

    pub(super) fn emit_record_type(&mut self, fields: &[TypedRecordField]) -> Document {
        let refs: Vec<&TypedRecordField> = fields.iter().collect();
        self.emit_record_type_fields(&refs)
    }

    pub(super) fn emit_union_type(&mut self, variants: &[TypedVariant]) -> Document {
        let mut docs = Vec::new();
        for (i, variant) in variants.iter().enumerate() {
            if i > 0 {
                docs.push(pretty::str(" | "));
            }
            if variant.fields.is_empty() {
                docs.push(pretty::str(format!(
                    "{{ {TAG_FIELD}: \"{}\" }}",
                    variant.name
                )));
            } else {
                let mut s = format!("{{ {TAG_FIELD}: \"{}\"", variant.name);
                for (fi, field) in variant.fields.iter().enumerate() {
                    s.push_str("; ");
                    if let Some(name) = &field.name {
                        s.push_str(name);
                    } else {
                        s.push_str(&type_layout::positional_field_name(
                            fi,
                            variant.fields.len(),
                        ));
                    }
                    s.push_str(": ");
                    let type_doc = self.emit_type_expr(&field.type_ann);
                    s.push_str(&Self::doc_to_string(&type_doc));
                }
                s.push_str(" }");
                docs.push(pretty::str(s));
            }
        }
        pretty::concat(docs)
    }

    pub(super) fn emit_string_literal_union_type(&self, variants: &[String]) -> Document {
        let parts: Vec<String> = variants
            .iter()
            .map(|v| format!("\"{}\"", escape_string(v)))
            .collect();
        pretty::str(parts.join(" | "))
    }

    pub(super) fn resolve_for_import_names(&self, decl: &ImportDecl) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(resolved) = self.ctx.resolved_imports.get(&decl.source) {
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
