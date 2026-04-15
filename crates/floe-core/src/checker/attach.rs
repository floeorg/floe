//! Converts an `UntypedProgram` (produced by `lower.rs`) into a
//! `TypedProgram` (consumed by `desugar` and `codegen`) by deep-cloning
//! the tree and attaching each expression's resolved `Arc<Type>` from
//! the checker's `ExprTypeMap`.
//!
//! This replaces the old in-place `annotate_types` mutator: under the
//! generic `Expr<T>` AST, `Expr<()>` and `Expr<Arc<Type>>` are
//! structurally distinct types, so codegen cannot be called on an
//! unchecked tree. The conversion runs once after type checking and
//! exists in one place so that the rest of the compiler doesn't need
//! to pattern-match `ExprKind` just to carry types forward.

use std::sync::Arc;

use crate::parser::ast::*;

use super::{ExprTypeMap, Type};

/// Convert an untyped program into a typed program, attaching each
/// expression's resolved type from `types`. Expressions missing from
/// the map fall back to `Type::Unknown` (this only happens for
/// synthetic nodes or unreachable subtrees after an error).
pub fn attach_types(program: UntypedProgram, types: &ExprTypeMap) -> TypedProgram {
    Attacher { types }.program(program)
}

/// Convert an untyped `TypeDecl` (e.g. from the resolver's list of
/// imported type declarations) into its typed equivalent. Used by
/// codegen to populate `type_defs` / `trait_decls` metadata when the
/// source is another `.fl` module's exports, where no `ExprTypeMap` is
/// available for the default expressions inside record fields. Those
/// defaults fall back to `Arc<Type::Unknown>` which is safe: the
/// metadata is only consulted for structural info (field names,
/// variant names, spread sources), not for the default expressions'
/// types.
pub fn attach_type_decl_shallow(decl: TypeDecl<()>) -> TypedTypeDecl {
    Attacher {
        types: &ExprTypeMap::new(),
    }
    .type_decl(decl)
}

/// Convert an untyped `TraitDecl` into its typed equivalent. See
/// `attach_type_decl_shallow` — defaults inside method bodies get
/// `Arc<Type::Unknown>` since no type map is available for imports.
pub fn attach_trait_decl_shallow(decl: TraitDecl<()>) -> TypedTraitDecl {
    Attacher {
        types: &ExprTypeMap::new(),
    }
    .trait_decl(decl)
}

struct Attacher<'a> {
    types: &'a ExprTypeMap,
}

impl Attacher<'_> {
    fn program(&self, program: UntypedProgram) -> TypedProgram {
        Program {
            items: program.items.into_iter().map(|i| self.item(i)).collect(),
            span: program.span,
        }
    }

    fn item(&self, item: Item<()>) -> Item<Arc<Type>> {
        Item {
            kind: self.item_kind(item.kind),
            span: item.span,
        }
    }

    fn item_kind(&self, kind: ItemKind<()>) -> ItemKind<Arc<Type>> {
        match kind {
            ItemKind::Import(d) => ItemKind::Import(d),
            ItemKind::ReExport(d) => ItemKind::ReExport(d),
            ItemKind::Const(c) => ItemKind::Const(self.const_decl(c)),
            ItemKind::Function(f) => ItemKind::Function(self.function_decl(f)),
            ItemKind::TypeDecl(t) => ItemKind::TypeDecl(self.type_decl(t)),
            ItemKind::ForBlock(b) => ItemKind::ForBlock(self.for_block(b)),
            ItemKind::TraitDecl(t) => ItemKind::TraitDecl(self.trait_decl(t)),
            ItemKind::TestBlock(t) => ItemKind::TestBlock(self.test_block(t)),
            ItemKind::Expr(e) => ItemKind::Expr(self.expr(e)),
        }
    }

    fn const_decl(&self, decl: ConstDecl<()>) -> ConstDecl<Arc<Type>> {
        ConstDecl {
            exported: decl.exported,
            binding: decl.binding,
            type_ann: decl.type_ann.map(|t| self.type_expr(t)),
            value: self.expr(decl.value),
        }
    }

    fn function_decl(&self, decl: FunctionDecl<()>) -> FunctionDecl<Arc<Type>> {
        FunctionDecl {
            exported: decl.exported,
            async_fn: decl.async_fn,
            name: decl.name,
            type_params: decl.type_params,
            params: decl.params.into_iter().map(|p| self.param(p)).collect(),
            return_type: decl.return_type.map(|t| self.type_expr(t)),
            body: Box::new(self.expr(*decl.body)),
        }
    }

    fn param(&self, param: Param<()>) -> Param<Arc<Type>> {
        Param {
            name: param.name,
            type_ann: param.type_ann.map(|t| self.type_expr(t)),
            default: param.default.map(|e| self.expr(e)),
            destructure: param.destructure,
            span: param.span,
        }
    }

    fn type_decl(&self, decl: TypeDecl<()>) -> TypeDecl<Arc<Type>> {
        TypeDecl {
            exported: decl.exported,
            opaque: decl.opaque,
            name: decl.name,
            type_params: decl.type_params,
            def: self.type_def(decl.def),
            deriving: decl.deriving,
        }
    }

    fn type_def(&self, def: TypeDef<()>) -> TypeDef<Arc<Type>> {
        match def {
            TypeDef::Record(entries) => {
                TypeDef::Record(entries.into_iter().map(|e| self.record_entry(e)).collect())
            }
            TypeDef::Union(variants) => {
                TypeDef::Union(variants.into_iter().map(|v| self.variant(v)).collect())
            }
            TypeDef::Alias(t) => TypeDef::Alias(self.type_expr(t)),
            TypeDef::StringLiteralUnion(vs) => TypeDef::StringLiteralUnion(vs),
        }
    }

    fn record_entry(&self, entry: RecordEntry<()>) -> RecordEntry<Arc<Type>> {
        match entry {
            RecordEntry::Field(f) => RecordEntry::Field(Box::new(self.record_field(*f))),
            RecordEntry::Spread(s) => RecordEntry::Spread(self.record_spread(s)),
        }
    }

    fn record_field(&self, field: RecordField<()>) -> RecordField<Arc<Type>> {
        RecordField {
            name: field.name,
            type_ann: self.type_expr(field.type_ann),
            default: field.default.map(|e| self.expr(e)),
            span: field.span,
        }
    }

    fn record_spread(&self, spread: RecordSpread<()>) -> RecordSpread<Arc<Type>> {
        RecordSpread {
            type_name: spread.type_name,
            type_expr: spread.type_expr.map(|t| self.type_expr(t)),
            span: spread.span,
        }
    }

    fn variant(&self, variant: Variant<()>) -> Variant<Arc<Type>> {
        Variant {
            name: variant.name,
            fields: variant
                .fields
                .into_iter()
                .map(|f| self.variant_field(f))
                .collect(),
            span: variant.span,
        }
    }

    fn variant_field(&self, field: VariantField<()>) -> VariantField<Arc<Type>> {
        VariantField {
            name: field.name,
            type_ann: self.type_expr(field.type_ann),
            span: field.span,
        }
    }

    fn trait_decl(&self, decl: TraitDecl<()>) -> TraitDecl<Arc<Type>> {
        TraitDecl {
            exported: decl.exported,
            name: decl.name,
            methods: decl
                .methods
                .into_iter()
                .map(|m| self.trait_method(m))
                .collect(),
            span: decl.span,
        }
    }

    fn trait_method(&self, method: TraitMethod<()>) -> TraitMethod<Arc<Type>> {
        TraitMethod {
            name: method.name,
            params: method.params.into_iter().map(|p| self.param(p)).collect(),
            return_type: method.return_type.map(|t| self.type_expr(t)),
            body: method.body.map(|e| self.expr(e)),
            span: method.span,
        }
    }

    fn for_block(&self, block: ForBlock<()>) -> ForBlock<Arc<Type>> {
        ForBlock {
            type_name: self.type_expr(block.type_name),
            trait_name: block.trait_name,
            functions: block
                .functions
                .into_iter()
                .map(|f| self.function_decl(f))
                .collect(),
            span: block.span,
        }
    }

    fn test_block(&self, block: TestBlock<()>) -> TestBlock<Arc<Type>> {
        TestBlock {
            name: block.name,
            body: block
                .body
                .into_iter()
                .map(|s| self.test_statement(s))
                .collect(),
            span: block.span,
        }
    }

    fn test_statement(&self, stmt: TestStatement<()>) -> TestStatement<Arc<Type>> {
        match stmt {
            TestStatement::Assert(e, span) => TestStatement::Assert(self.expr(e), span),
            TestStatement::Expr(e) => TestStatement::Expr(self.expr(e)),
        }
    }

    fn type_expr(&self, expr: TypeExpr<()>) -> TypeExpr<Arc<Type>> {
        TypeExpr {
            kind: self.type_expr_kind(expr.kind),
            span: expr.span,
        }
    }

    fn type_expr_kind(&self, kind: TypeExprKind<()>) -> TypeExprKind<Arc<Type>> {
        match kind {
            TypeExprKind::Named {
                name,
                type_args,
                bounds,
            } => TypeExprKind::Named {
                name,
                type_args: type_args.into_iter().map(|t| self.type_expr(t)).collect(),
                bounds,
            },
            TypeExprKind::Record(fields) => {
                TypeExprKind::Record(fields.into_iter().map(|f| self.record_field(f)).collect())
            }
            TypeExprKind::Function {
                params,
                return_type,
            } => TypeExprKind::Function {
                params: params.into_iter().map(|t| self.type_expr(t)).collect(),
                return_type: Box::new(self.type_expr(*return_type)),
            },
            TypeExprKind::Array(t) => TypeExprKind::Array(Box::new(self.type_expr(*t))),
            TypeExprKind::Tuple(ts) => {
                TypeExprKind::Tuple(ts.into_iter().map(|t| self.type_expr(t)).collect())
            }
            TypeExprKind::TypeOf(name) => TypeExprKind::TypeOf(name),
            TypeExprKind::Intersection(ts) => {
                TypeExprKind::Intersection(ts.into_iter().map(|t| self.type_expr(t)).collect())
            }
            TypeExprKind::StringLiteral(s) => TypeExprKind::StringLiteral(s),
        }
    }

    fn expr(&self, expr: UntypedExpr) -> TypedExpr {
        let ty = self.types.get(&expr.id).cloned().unwrap_or(Type::Unknown);
        Expr {
            id: expr.id,
            kind: self.expr_kind(expr.kind),
            ty: Arc::new(ty),
            span: expr.span,
        }
    }

    #[allow(clippy::boxed_local)] // Callers always hand over a Box from ExprKind.
    fn boxed_expr(&self, expr: Box<UntypedExpr>) -> Box<TypedExpr> {
        Box::new(self.expr(*expr))
    }

    fn expr_kind(&self, kind: ExprKind<()>) -> ExprKind<Arc<Type>> {
        match kind {
            ExprKind::Number(n) => ExprKind::Number(n),
            ExprKind::String(s) => ExprKind::String(s),
            ExprKind::TemplateLiteral(parts) => ExprKind::TemplateLiteral(
                parts.into_iter().map(|p| self.template_part(p)).collect(),
            ),
            ExprKind::Bool(b) => ExprKind::Bool(b),
            ExprKind::Identifier(n) => ExprKind::Identifier(n),
            ExprKind::Placeholder => ExprKind::Placeholder,
            ExprKind::Binary { left, op, right } => ExprKind::Binary {
                left: self.boxed_expr(left),
                op,
                right: self.boxed_expr(right),
            },
            ExprKind::Unary { op, operand } => ExprKind::Unary {
                op,
                operand: self.boxed_expr(operand),
            },
            ExprKind::Pipe { left, right } => ExprKind::Pipe {
                left: self.boxed_expr(left),
                right: self.boxed_expr(right),
            },
            ExprKind::Unwrap(e) => ExprKind::Unwrap(self.boxed_expr(e)),
            ExprKind::Call {
                callee,
                type_args,
                args,
            } => ExprKind::Call {
                callee: self.boxed_expr(callee),
                type_args: type_args.into_iter().map(|t| self.type_expr(t)).collect(),
                args: args.into_iter().map(|a| self.arg(a)).collect(),
            },
            ExprKind::Construct {
                type_name,
                spread,
                args,
            } => ExprKind::Construct {
                type_name,
                spread: spread.map(|s| self.boxed_expr(s)),
                args: args.into_iter().map(|a| self.arg(a)).collect(),
            },
            ExprKind::Member { object, field } => ExprKind::Member {
                object: self.boxed_expr(object),
                field,
            },
            ExprKind::Index { object, index } => ExprKind::Index {
                object: self.boxed_expr(object),
                index: self.boxed_expr(index),
            },
            ExprKind::Arrow {
                async_fn,
                params,
                body,
            } => ExprKind::Arrow {
                async_fn,
                params: params.into_iter().map(|p| self.param(p)).collect(),
                body: self.boxed_expr(body),
            },
            ExprKind::Match { subject, arms } => ExprKind::Match {
                subject: self.boxed_expr(subject),
                arms: arms.into_iter().map(|a| self.match_arm(a)).collect(),
            },
            ExprKind::Value(e) => ExprKind::Value(self.boxed_expr(e)),
            ExprKind::Clear => ExprKind::Clear,
            ExprKind::Unchanged => ExprKind::Unchanged,
            ExprKind::Parse { type_arg, value } => ExprKind::Parse {
                type_arg: self.type_expr(type_arg),
                value: self.boxed_expr(value),
            },
            ExprKind::Mock {
                type_arg,
                overrides,
            } => ExprKind::Mock {
                type_arg: self.type_expr(type_arg),
                overrides: overrides.into_iter().map(|a| self.arg(a)).collect(),
            },
            ExprKind::Todo => ExprKind::Todo,
            ExprKind::Unreachable => ExprKind::Unreachable,
            ExprKind::Unit => ExprKind::Unit,
            ExprKind::Jsx(el) => ExprKind::Jsx(self.jsx_element(el)),
            ExprKind::Block(items) => {
                ExprKind::Block(items.into_iter().map(|i| self.item(i)).collect())
            }
            ExprKind::Collect(items) => {
                ExprKind::Collect(items.into_iter().map(|i| self.item(i)).collect())
            }
            ExprKind::Grouped(e) => ExprKind::Grouped(self.boxed_expr(e)),
            ExprKind::Array(es) => ExprKind::Array(es.into_iter().map(|e| self.expr(e)).collect()),
            ExprKind::Object(fields) => ExprKind::Object(
                fields
                    .into_iter()
                    .map(|(name, e)| (name, self.expr(e)))
                    .collect(),
            ),
            ExprKind::Tuple(es) => ExprKind::Tuple(es.into_iter().map(|e| self.expr(e)).collect()),
            ExprKind::Spread(e) => ExprKind::Spread(self.boxed_expr(e)),
            ExprKind::DotShorthand { field, predicate } => ExprKind::DotShorthand {
                field,
                predicate: predicate.map(|(op, e)| (op, self.boxed_expr(e))),
            },
        }
    }

    fn template_part(&self, part: TemplatePart<()>) -> TemplatePart<Arc<Type>> {
        match part {
            TemplatePart::Raw(s) => TemplatePart::Raw(s),
            TemplatePart::Expr(e) => TemplatePart::Expr(self.expr(e)),
        }
    }

    fn arg(&self, arg: Arg<()>) -> Arg<Arc<Type>> {
        match arg {
            Arg::Positional(e) => Arg::Positional(self.expr(e)),
            Arg::Named { label, value } => Arg::Named {
                label,
                value: self.expr(value),
            },
        }
    }

    fn match_arm(&self, arm: MatchArm<()>) -> MatchArm<Arc<Type>> {
        MatchArm {
            pattern: arm.pattern,
            guard: arm.guard.map(|e| self.expr(e)),
            body: self.expr(arm.body),
            span: arm.span,
        }
    }

    fn jsx_element(&self, el: JsxElement<()>) -> JsxElement<Arc<Type>> {
        JsxElement {
            kind: self.jsx_element_kind(el.kind),
            span: el.span,
        }
    }

    fn jsx_element_kind(&self, kind: JsxElementKind<()>) -> JsxElementKind<Arc<Type>> {
        match kind {
            JsxElementKind::Element {
                name,
                props,
                children,
                self_closing,
            } => JsxElementKind::Element {
                name,
                props: props.into_iter().map(|p| self.jsx_prop(p)).collect(),
                children: children.into_iter().map(|c| self.jsx_child(c)).collect(),
                self_closing,
            },
            JsxElementKind::Fragment { children } => JsxElementKind::Fragment {
                children: children.into_iter().map(|c| self.jsx_child(c)).collect(),
            },
        }
    }

    fn jsx_prop(&self, prop: JsxProp<()>) -> JsxProp<Arc<Type>> {
        match prop {
            JsxProp::Named { name, value, span } => JsxProp::Named {
                name,
                value: value.map(|e| self.expr(e)),
                span,
            },
            JsxProp::Spread { expr, span } => JsxProp::Spread {
                expr: self.expr(expr),
                span,
            },
        }
    }

    fn jsx_child(&self, child: JsxChild<()>) -> JsxChild<Arc<Type>> {
        match child {
            JsxChild::Text(s) => JsxChild::Text(s),
            JsxChild::Expr(e) => JsxChild::Expr(self.expr(e)),
            JsxChild::Element(el) => JsxChild::Element(self.jsx_element(el)),
        }
    }
}
