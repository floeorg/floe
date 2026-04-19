use std::collections::HashSet;

use crate::parser::ast::*;

use super::super::for_block_fn_name;
use super::generator::TypeScriptGenerator;

impl<'a> TypeScriptGenerator<'a> {
    // ── Declaration Stub Generation (.d.ts) ───────────────────────

    /// Generate a `.d.ts` declaration stub from the program AST.
    pub(super) fn generate_dts(&mut self, program: &TypedProgram) -> String {
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
                ItemKind::ReExport(decl) => {
                    if !first {
                        out.push('\n');
                    }
                    first = false;
                    self.emit_dts_reexport(&mut out, decl);
                }
                ItemKind::TraitDecl(_) | ItemKind::TestBlock(_) | ItemKind::Expr(_) => {}
            }
        }

        if !out.is_empty() {
            out.push('\n');
        }
        out
    }

    fn emit_dts_reexport(&self, out: &mut String, decl: &ReExportDecl) {
        out.push_str("export { ");
        for (i, spec) in decl.specifiers.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&spec.name);
            if let Some(alias) = &spec.alias {
                out.push_str(" as ");
                out.push_str(alias);
            }
        }
        out.push_str(&format!(" }} from \"{}\";", decl.source));
    }

    fn emit_dts_import(&self, out: &mut String, decl: &ImportDecl) {
        if decl.specifiers.is_empty() && decl.for_specifiers.is_empty() {
            if let Some(resolved) = self.ctx.resolved_imports.get(&decl.source) {
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
            let type_only_names: HashSet<String> =
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

    fn emit_dts_type_decl(&mut self, out: &mut String, decl: &TypedTypeDecl) {
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

        let type_doc = match &decl.def {
            TypeDef::Record(entries) => self.emit_record_type_entries(entries),
            TypeDef::Union(variants) => self.emit_union_type(variants),
            TypeDef::StringLiteralUnion(variants) => self.emit_string_literal_union_type(variants),
            TypeDef::Alias(type_expr) => self.emit_type_expr(type_expr),
        };
        out.push_str(&Self::doc_to_string(&type_doc));
        out.push(';');

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

    fn emit_dts_function(&mut self, out: &mut String, decl: &TypedFunctionDecl) {
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
            for (i, tp) in decl.type_params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&tp.name);
                if !tp.bounds.is_empty() {
                    out.push_str(" extends ");
                    out.push_str(&tp.bounds.join(" & "));
                }
            }
            out.push('>');
        }
        out.push('(');
        let params_doc = self.emit_params(&decl.params);
        out.push_str(&Self::doc_to_string(&params_doc));
        out.push(')');

        if let Some(ret) = &decl.return_type {
            out.push_str(": ");
            let needs_promise_wrap = decl.async_fn
                && !matches!(&ret.kind, TypeExprKind::Named { name, type_args, .. } if name == "Promise" && !type_args.is_empty());
            let type_doc = self.emit_type_expr(ret);
            let type_str = Self::doc_to_string(&type_doc);
            if needs_promise_wrap {
                out.push_str(&format!("Promise<{type_str}>"));
            } else {
                out.push_str(&type_str);
            }
        }
        out.push(';');
    }

    fn emit_dts_const(&mut self, out: &mut String, decl: &TypedConstDecl) {
        match &decl.binding {
            ConstBinding::Name(name) => {
                out.push_str("export declare const ");
                out.push_str(name);
                if let Some(type_ann) = &decl.type_ann {
                    out.push_str(": ");
                    let type_doc = self.emit_type_expr(type_ann);
                    out.push_str(&Self::doc_to_string(&type_doc));
                } else {
                    out.push_str(": any");
                }
                out.push(';');
            }
            ConstBinding::Tuple(names) => {
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
        &mut self,
        out: &mut String,
        func: &TypedFunctionDecl,
        for_type: &TypedTypeExpr,
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
                let type_doc = self.emit_type_expr(for_type);
                out.push_str(&Self::doc_to_string(&type_doc));
            } else if let Some(type_ann) = &param.type_ann {
                out.push_str(": ");
                let type_doc = self.emit_type_expr(type_ann);
                out.push_str(&Self::doc_to_string(&type_doc));
            }
        }

        out.push(')');

        if let Some(ret) = &func.return_type {
            out.push_str(": ");
            let needs_promise_wrap = func.async_fn
                && !matches!(&ret.kind, TypeExprKind::Named { name, type_args, .. } if name == "Promise" && !type_args.is_empty());
            let type_doc = self.emit_type_expr(ret);
            let type_str = Self::doc_to_string(&type_doc);
            if needs_promise_wrap {
                out.push_str(&format!("Promise<{type_str}>"));
            } else {
                out.push_str(&type_str);
            }
        }
        out.push(';');
    }
}
