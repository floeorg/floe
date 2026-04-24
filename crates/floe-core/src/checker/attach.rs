//! Converts an `UntypedProgram` (produced by `lower.rs`) into a
//! `TypedProgram` (consumed by `codegen`) by deep-cloning the tree and
//! attaching each expression's resolved `Arc<Type>` from the checker's
//! `ExprTypeMap`.
//!
//! Under the generic `Expr<T>` AST, `Expr<()>` and `Expr<Arc<Type>>`
//! are structurally distinct types, so codegen cannot be called on an
//! unchecked tree. The conversion runs once after type checking and
//! lives in one place so the rest of the compiler doesn't need to
//! pattern-match `ExprKind` just to carry types forward.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use crate::parser::ast::*;
use crate::resolve::ResolvedImports;

use super::{ExprTypeMap, Type, UNKNOWN};

/// Shared empty `ExprTypeMap` used by the shallow import converters.
static EMPTY_TYPES: LazyLock<ExprTypeMap> = LazyLock::new(ExprTypeMap::new);

/// Shared empty invalid-exprs set for shallow import converters.
static EMPTY_INVALID: LazyLock<HashSet<ExprId>> = LazyLock::new(HashSet::new);

/// Convert an untyped program into a typed program, attaching each
/// expression's resolved type from `types`. Expressions whose IDs
/// appear in `invalid_exprs` are replaced with `ExprKind::Invalid`
/// so codegen and downstream passes skip the broken subtree.
/// Expressions missing from the type map but NOT in the invalid set
/// fall back to the shared `UNKNOWN` sentinel (codegen-synthetic
/// nodes and unreachable subtrees).
pub fn attach_types(
    program: UntypedProgram,
    types: &ExprTypeMap,
    invalid_exprs: &HashSet<ExprId>,
) -> TypedProgram {
    Attacher {
        types,
        invalid_exprs,
    }
    .program(program)
}

/// Run the full post-check pipeline in one place so every call site
/// (the CLI build/check/watch/test paths, the LSP, codegen snapshot
/// tests, the wasm playground) stays in lockstep. Without this helper
/// each caller stitches `mark_async_functions` + `desugar_program` +
/// `attach_types` together by hand and one of them always forgets
/// `mark_async_functions`, which silently diverges async-marking
/// between the CLI and the playground.
///
/// Ordering: `mark_async_functions` and `desugar` both walk the still
/// untyped tree (neither reads `.ty`), then `attach_types` converts
/// the tree into its typed form for codegen.
pub fn lower_to_typed(
    mut program: UntypedProgram,
    expr_types: &ExprTypeMap,
    invalid_exprs: &HashSet<ExprId>,
    resolved: &HashMap<String, ResolvedImports>,
) -> TypedProgram {
    crate::checker::mark_async_functions(&mut program);
    crate::desugar::desugar_program(&mut program, resolved);
    attach_types(program, expr_types, invalid_exprs)
}

/// Convert an untyped `TypeDecl` (e.g. from the resolver's list of
/// imported type declarations) into its typed equivalent. Used by
/// codegen to populate `type_defs` metadata when the source is another
/// `.fl` module's exports, where no `ExprTypeMap` is available for the
/// default expressions inside record fields. Defaults fall back to the
/// shared `UNKNOWN` sentinel — safe because the metadata is only
/// consulted for structural info (field names, variant names, spread
/// sources), not for the default expressions' types.
pub fn attach_type_decl_shallow(decl: &TypeDecl<()>) -> TypedTypeDecl {
    Attacher {
        types: &EMPTY_TYPES,
        invalid_exprs: &EMPTY_INVALID,
    }
    .type_decl(decl.clone())
}

/// Convert an untyped `TraitDecl` into its typed equivalent. See
/// `attach_type_decl_shallow` — defaults inside method bodies get the
/// shared `UNKNOWN` sentinel since no type map is available for
/// imports.
pub fn attach_trait_decl_shallow(decl: &TraitDecl<()>) -> TypedTraitDecl {
    Attacher {
        types: &EMPTY_TYPES,
        invalid_exprs: &EMPTY_INVALID,
    }
    .trait_decl(decl.clone())
}

struct Attacher<'a> {
    types: &'a ExprTypeMap,
    invalid_exprs: &'a HashSet<ExprId>,
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
            ItemKind::DefaultExport(d) => ItemKind::DefaultExport(d),
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
            trait_name_span: block.trait_name_span,
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
            TestStatement::Let(decl) => TestStatement::Let(self.const_decl(decl)),
            TestStatement::Expr(e) => TestStatement::Expr(self.expr(e)),
        }
    }

    fn type_expr(&self, expr: TypeExpr<()>) -> TypeExpr<Arc<Type>> {
        TypeExpr {
            kind: self.type_expr_kind(expr.kind),
            span: expr.span,
        }
    }

    fn fn_type_param(&self, param: FnTypeParam<()>) -> FnTypeParam<Arc<Type>> {
        FnTypeParam {
            label: param.label,
            type_ann: self.type_expr(param.type_ann),
            span: param.span,
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
                params: params.into_iter().map(|p| self.fn_type_param(p)).collect(),
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
        // If the checker flagged this expression as invalid (type error
        // was already emitted), replace the whole subtree with Invalid
        // so codegen doesn't try to emit code for a broken tree.
        if self.invalid_exprs.contains(&expr.id) {
            let ty = self
                .types
                .get(&expr.id)
                .cloned()
                .unwrap_or_else(|| Arc::clone(&UNKNOWN));
            return Expr {
                id: expr.id,
                kind: ExprKind::Invalid,
                ty,
                span: expr.span,
            };
        }

        // Cheap: `Arc::clone` bumps a refcount. The fallback path hits the
        // shared `UNKNOWN` sentinel so codegen-synthetic nodes and
        // post-error subtrees don't allocate a fresh `Arc` each.
        let ty = self
            .types
            .get(&expr.id)
            .cloned()
            .unwrap_or_else(|| Arc::clone(&UNKNOWN));
        Expr {
            id: expr.id,
            kind: self.expr_kind(expr.kind),
            ty,
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
            ExprKind::TaggedTemplate { tag, parts } => ExprKind::TaggedTemplate {
                tag: self.boxed_expr(tag),
                parts: parts.into_iter().map(|p| self.template_part(p)).collect(),
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
            ExprKind::Invalid => ExprKind::Invalid,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::span::Span;

    fn span() -> Span {
        Span {
            start: 0,
            end: 0,
            line: 0,
            column: 0,
        }
    }

    #[test]
    fn empty_program_roundtrips() {
        let program = Program {
            items: Vec::new(),
            span: span(),
        };
        let typed = attach_types(
            program,
            &ExprTypeMap::new(),
            &std::collections::HashSet::new(),
        );
        assert!(typed.items.is_empty());
    }

    #[test]
    fn missing_expr_id_falls_back_to_unknown_sentinel() {
        // An expression whose id is not in the type map must survive
        // conversion and carry the shared `UNKNOWN` Arc. This is the
        // codegen-synthetic and post-error subtree path.
        let expr = Expr {
            id: ExprId(0),
            kind: ExprKind::Number("1".to_string()),
            ty: (),
            span: span(),
        };
        let program = Program {
            items: vec![Item {
                kind: ItemKind::Expr(expr),
                span: span(),
            }],
            span: span(),
        };
        let typed = attach_types(
            program,
            &ExprTypeMap::new(),
            &std::collections::HashSet::new(),
        );
        let ItemKind::Expr(e) = &typed.items[0].kind else {
            panic!("expected Expr item");
        };
        assert!(matches!(&*e.ty, Type::Unknown));
        // Confirm it's the shared sentinel, not a fresh allocation.
        assert!(Arc::ptr_eq(&e.ty, &*UNKNOWN));
    }

    #[test]
    fn resolved_expr_id_gets_mapped_type() {
        let mut types = ExprTypeMap::new();
        types.insert(ExprId(0), Arc::new(Type::Number));
        let expr = Expr {
            id: ExprId(0),
            kind: ExprKind::Number("1".to_string()),
            ty: (),
            span: span(),
        };
        let program = Program {
            items: vec![Item {
                kind: ItemKind::Expr(expr),
                span: span(),
            }],
            span: span(),
        };
        let typed = attach_types(program, &types, &std::collections::HashSet::new());
        let ItemKind::Expr(e) = &typed.items[0].kind else {
            panic!("expected Expr item");
        };
        assert!(matches!(&*e.ty, Type::Number));
    }

    #[test]
    fn attach_type_decl_shallow_preserves_structure() {
        let decl = TypeDecl::<()> {
            exported: true,
            opaque: false,
            name: "Point".to_string(),
            type_params: vec![],
            def: TypeDef::Record(vec![RecordEntry::Field(Box::new(RecordField {
                name: "x".to_string(),
                type_ann: TypeExpr {
                    kind: TypeExprKind::Named {
                        name: "number".to_string(),
                        type_args: vec![],
                        bounds: vec![],
                    },
                    span: span(),
                },
                default: None,
                span: span(),
            }))]),
        };
        let typed = attach_type_decl_shallow(&decl);
        assert_eq!(typed.name, "Point");
        assert_eq!(typed.def.record_fields().len(), 1);
    }
}
