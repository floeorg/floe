use crate::parser::ast::*;
use crate::pretty::{self, Document};

use super::super::has_placeholder_arg;
use super::generator::TypeScriptGenerator;

impl<'a> TypeScriptGenerator<'a> {
    // ── Pipe Lowering ────────────────────────────────────────────

    pub(super) fn try_emit_stdlib_call(
        &mut self,
        callee: &TypedExpr,
        args: &[TypedArg],
    ) -> Option<String> {
        let ExprKind::Member { object, field } = &callee.kind else {
            return None;
        };

        // Qualified stdlib call: `Array.map(arr, f)` — module is a bare name
        // that matches a stdlib module.
        if let ExprKind::Identifier(module) = &object.kind
            && let Some(stdlib_fn) = self.ctx.stdlib.lookup(module, field)
        {
            let template = stdlib_fn.codegen;
            let arg_strings = self.emit_arg_strings(args);
            return Some(self.apply_stdlib_template(template, &arg_strings));
        }

        // Receiver-style stdlib call: `arr.map(f)` where `arr: Array<T>` —
        // dispatch through the receiver's type. The receiver becomes the
        // first stdlib argument so the emitted template lines up with the
        // pipe form.
        if let Some(module) = crate::type_layout::type_to_stdlib_module(&object.ty)
            && let Some(stdlib_fn) = self.ctx.stdlib.lookup(module, field)
        {
            let template = stdlib_fn.codegen.to_string();
            let object_str = self.emit_expr_string(object);
            let mut arg_strings = vec![object_str];
            arg_strings.extend(self.emit_arg_strings(args));
            return Some(self.apply_stdlib_template(&template, &arg_strings));
        }

        None
    }

    fn try_emit_stdlib_pipe(
        &mut self,
        left: &TypedExpr,
        callee: &TypedExpr,
        extra_args: &[TypedArg],
    ) -> Option<String> {
        if let ExprKind::Member { object, field } = &callee.kind
            && let ExprKind::Identifier(module) = &object.kind
            && let Some(stdlib_fn) = self.ctx.stdlib.lookup(module, field)
        {
            let template = stdlib_fn.codegen;
            let left_str = self.emit_expr_string(left);
            let mut arg_strings = vec![left_str];
            arg_strings.extend(self.emit_arg_strings(extra_args));
            Some(self.apply_stdlib_template(template, &arg_strings))
        } else {
            None
        }
    }

    fn try_emit_for_block_pipe(
        &mut self,
        left: &TypedExpr,
        type_name: &str,
        field: &str,
        args: &[TypedArg],
    ) -> Option<Document> {
        let mangled = self
            .ctx
            .for_block_fns
            .get(&(type_name.to_string(), field.to_string()))?;
        let name = self
            .import_aliases
            .get(mangled)
            .cloned()
            .unwrap_or_else(|| mangled.clone());
        let mut docs = vec![pretty::str(&name), pretty::str("(")];
        docs.push(self.emit_expr(left));
        if !args.is_empty() {
            docs.push(pretty::str(", "));
            docs.push(self.emit_args(args));
        }
        docs.push(pretty::str(")"));
        Some(pretty::concat(docs))
    }

    fn try_emit_bare_stdlib_pipe(
        &mut self,
        left: &TypedExpr,
        callee: &TypedExpr,
        extra_args: &[TypedArg],
    ) -> Option<String> {
        if let ExprKind::Identifier(name) = &callee.kind {
            if self.ctx.local_names.contains(name.as_str())
                && self.ctx.stdlib.lookup_by_name(name).is_empty()
            {
                return None;
            }

            let stdlib_fn = match crate::type_layout::type_to_stdlib_module(&left.ty) {
                Some(module) => self
                    .ctx
                    .stdlib
                    .lookup(module, name)
                    .or_else(|| self.ctx.stdlib.lookup_by_name(name).into_iter().next()),
                None => self.ctx.stdlib.lookup_by_name(name).into_iter().next(),
            }?;

            let template = stdlib_fn.codegen.to_string();
            let left_str = self.emit_expr_string(left);
            let mut arg_strings = vec![left_str];
            arg_strings.extend(self.emit_arg_strings(extra_args));
            return Some(self.apply_stdlib_template(&template, &arg_strings));
        }
        None
    }

    fn try_emit_trait_bounded_pipe(
        &mut self,
        left: &TypedExpr,
        callee: &TypedExpr,
        args: &[TypedArg],
    ) -> Option<Document> {
        let crate::checker::Type::Named(type_param_name) = &*left.ty else {
            return None;
        };
        let bounds = self
            .current_type_param_bounds
            .get(type_param_name.as_str())?;
        let ExprKind::Identifier(method_name) = &callee.kind else {
            return None;
        };
        for bound_trait in bounds {
            let has_method = self
                .ctx
                .trait_decls
                .get(bound_trait.as_str())
                .is_some_and(|td| td.methods.iter().any(|m| m.name == *method_name));
            if has_method {
                return Some(pretty::concat([
                    self.emit_expr(left),
                    pretty::str("."),
                    pretty::str(method_name),
                    pretty::str("("),
                    self.emit_args(args),
                    pretty::str(")"),
                ]));
            }
        }
        None
    }

    pub(super) fn emit_pipe(&mut self, left: &TypedExpr, right: &TypedExpr) -> Document {
        match &right.kind {
            ExprKind::Call { callee, args, .. } if !has_placeholder_arg(args) => {
                if let Some(output) = self.try_emit_stdlib_pipe(left, callee, args) {
                    return pretty::str(output);
                }
                if let Some(output) = self.try_emit_bare_stdlib_pipe(left, callee, args) {
                    return pretty::str(output);
                }
                if let Some(doc) = self.try_emit_trait_bounded_pipe(left, callee, args) {
                    return doc;
                }
                if let ExprKind::Member { object, field } = &callee.kind
                    && let ExprKind::Identifier(type_name) = &object.kind
                    && let Some(doc) = self.try_emit_for_block_pipe(left, type_name, field, args)
                {
                    return doc;
                }
                if let ExprKind::Identifier(name) = &callee.kind
                    && let Some(mangled) = self
                        .ctx
                        .lookup_for_block_fn_by_name(name, &self.import_aliases)
                {
                    let mut docs = vec![pretty::str(&mangled), pretty::str("(")];
                    docs.push(self.emit_expr(left));
                    if !args.is_empty() {
                        docs.push(pretty::str(", "));
                        docs.push(self.emit_args(args));
                    }
                    docs.push(pretty::str(")"));
                    return pretty::concat(docs);
                }
                // Fall through to normal call
                let callee_alias = if let ExprKind::Identifier(name) = &callee.kind {
                    self.import_aliases.get(name.as_str()).cloned()
                } else {
                    None
                };
                let callee_doc = if let Some(alias) = callee_alias {
                    pretty::str(alias)
                } else {
                    self.emit_expr(callee)
                };
                let mut docs = vec![callee_doc, pretty::str("(")];
                docs.push(self.emit_expr(left));
                if !args.is_empty() {
                    docs.push(pretty::str(", "));
                    docs.push(self.emit_args(args));
                }
                docs.push(pretty::str(")"));
                pretty::concat(docs)
            }
            ExprKind::Member { object, field } => {
                if let Some(output) = self.try_emit_stdlib_pipe(left, right, &[]) {
                    return pretty::str(output);
                }
                if let ExprKind::Identifier(type_name) = &object.kind
                    && let Some(doc) = self.try_emit_for_block_pipe(left, type_name, field, &[])
                {
                    return doc;
                }
                pretty::concat([
                    self.emit_expr(right),
                    pretty::str("("),
                    self.emit_expr(left),
                    pretty::str(")"),
                ])
            }
            ExprKind::Call { callee, args, .. } if has_placeholder_arg(args) => {
                let mut docs = vec![self.emit_expr(callee), pretty::str("(")];
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        docs.push(pretty::str(", "));
                    }
                    match arg {
                        Arg::Positional(expr) if matches!(expr.kind, ExprKind::Placeholder) => {
                            docs.push(self.emit_expr(left));
                        }
                        Arg::Positional(expr) => docs.push(self.emit_expr(expr)),
                        Arg::Named { value, .. } => {
                            if matches!(value.kind, ExprKind::Placeholder) {
                                docs.push(self.emit_expr(left));
                            } else {
                                docs.push(self.emit_expr(value));
                            }
                        }
                    }
                }
                docs.push(pretty::str(")"));
                pretty::concat(docs)
            }
            ExprKind::Parse { type_arg, value } if matches!(value.kind, ExprKind::Placeholder) => {
                let substituted = TypedExpr::synthetic_typed(
                    ExprKind::Parse {
                        type_arg: type_arg.clone(),
                        value: Box::new(left.clone()),
                    },
                    right.span,
                );
                self.emit_expr(&substituted)
            }
            ExprKind::Identifier(name) => {
                if let Some(output) = self.try_emit_bare_stdlib_pipe(left, right, &[]) {
                    return pretty::str(output);
                }
                if let Some(mangled) = self
                    .ctx
                    .lookup_for_block_fn_by_name(name, &self.import_aliases)
                {
                    return pretty::concat([
                        pretty::str(&mangled),
                        pretty::str("("),
                        self.emit_expr(left),
                        pretty::str(")"),
                    ]);
                }
                let alias = self.import_aliases.get(name.as_str()).cloned();
                let callee_doc = if let Some(alias) = alias {
                    pretty::str(alias)
                } else {
                    self.emit_expr(right)
                };
                pretty::concat([
                    callee_doc,
                    pretty::str("("),
                    self.emit_expr(left),
                    pretty::str(")"),
                ])
            }
            _ => pretty::concat([
                self.emit_expr(right),
                pretty::str("("),
                self.emit_expr(left),
                pretty::str(")"),
            ]),
        }
    }
}
