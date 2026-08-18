use std::collections::{HashMap, HashSet};

use crate::checker::Type;
use crate::parser::ast::{
    ForBlock, ItemKind, TypeDef, TypeExprKind, TypedForBlock, TypedProgram, TypedTraitDecl,
    TypedTypeDecl, TypedTypeDef, TypedTypeExpr, file_scope_names,
};
use crate::pretty::{self, Document};
use crate::resolve::ResolvedImports;
use crate::stdlib::StdlibRegistry;
use crate::type_layout;

use super::super::{
    CodegenOutput, DEEP_EQUAL_FN, FnShape, GlobalName, checker_agrees, collect_constructor_names,
    collect_value_used_names, for_block_base_type_name, for_block_fn_name, written_annotation_name,
};

// ── Runtime codegen constants ───────────────────────────────────

/// `todo` expression — throws "not implemented" at runtime.
pub(super) const THROW_NOT_IMPLEMENTED: &str =
    "(() => { throw new Error(\"not implemented\"); })()";

/// `unreachable` expression — throws "unreachable" at runtime.
pub(super) const THROW_UNREACHABLE: &str = "(() => { throw new Error(\"unreachable\"); })()";

/// Fallback for non-exhaustive match — throws at runtime.
pub(super) const THROW_NON_EXHAUSTIVE: &str =
    "(() => { throw new Error(\"non-exhaustive match\"); })()";

/// Mock placeholder for function types — throws when called.
pub(super) const THROW_MOCK_FUNCTION: &str = "(() => { throw new Error(\"mock function\"); })";

/// Maximum line width for pretty-printing. The current codegen produces
/// fixed-layout output: groups are built to never break, so the width
/// limit is only a ceiling for `fits()` — setting it to a very large
/// value effectively disables wrapping. Keep it within `isize::MAX`
/// so `limit as isize` in `pretty::render` stays positive.
const PRINT_WIDTH: usize = 1_000_000;

/// A for-block function, as codegen's file-global name maps hold it.
#[derive(Clone)]
pub(crate) struct ForBlockFn {
    /// The emitted name, `Entry__double`.
    pub mangled: String,
    /// The signature the declaration writes, which the guard compares
    /// against the type the checker resolved at the use site.
    pub shape: FnShape,
}

impl ForBlockFn {
    /// The emitted name, after import aliasing.
    pub(super) fn emitted_name(&self, import_aliases: &HashMap<String, String>) -> String {
        import_aliases
            .get(&self.mangled)
            .cloned()
            .unwrap_or_else(|| self.mangled.clone())
    }
}

/// Read-only type metadata collected during the first pass.
/// Borrowed by the generator — no cloning needed for sub-expressions.
pub(crate) struct TypeContext {
    pub stdlib: StdlibRegistry,
    /// Every union variant in the file: `variant_name` → (union name, field
    /// names). An empty field list marks a unit variant.
    pub variant_info: HashMap<String, (String, Vec<String>)>,
    pub type_defs: HashMap<String, TypedTypeDef>,
    pub local_names: HashSet<String>,
    pub resolved_imports: HashMap<String, ResolvedImports>,
    pub test_mode: bool,
    pub value_used_names: HashSet<String>,
    pub for_block_fns: HashMap<(String, String), ForBlockFn>,
    /// Bare-name index into `for_block_fns`: maps `fn_name` → every
    /// for-block that declares it, in registration order. Two for-blocks
    /// may declare one method name on different types, and the checker
    /// picks between them by the receiver, so codegen holds them all and
    /// picks the same way. Keeping only the first registration made
    /// codegen answer `A__show` for a call the checker resolved to `B`.
    pub for_block_fns_by_name: HashMap<String, Vec<ForBlockFn>>,
    pub for_block_type_names: HashSet<String>,
    pub constructor_used_names: HashSet<String>,
    pub trait_decls: HashMap<String, TypedTraitDecl>,
    pub type_trait_impls: HashMap<String, Vec<String>>,
    pub traits_needing_interface: HashSet<String>,
    /// All local `for T: Trait { ... }` blocks grouped by the implementing
    /// type name. Used to emit a single `T__make` factory per type that
    /// wires up every trait method, rather than one factory per for-block
    /// (which would collide when a type has multiple trait impls).
    pub trait_impl_blocks: HashMap<String, Vec<TypedForBlock>>,
    /// Mangled names of every for-block method this file declares itself.
    /// An import must not bring in a name the file already declares, or
    /// TypeScript reports a duplicate identifier.
    pub local_for_block_fn_names: HashSet<String>,
}

impl TypeContext {
    /// Build the type context from a program and resolved imports.
    /// Runs the first pass: collects variant info, local names, trait data, etc.
    pub fn from_program(
        program: &TypedProgram,
        resolved_imports: &HashMap<String, ResolvedImports>,
        test_mode: bool,
    ) -> Self {
        let mut ctx = Self {
            stdlib: StdlibRegistry::new(),
            variant_info: HashMap::new(),
            type_defs: HashMap::new(),
            local_names: HashSet::new(),
            resolved_imports: resolved_imports.clone(),
            test_mode,
            value_used_names: collect_value_used_names(program),
            for_block_fns: HashMap::new(),
            for_block_fns_by_name: HashMap::new(),
            for_block_type_names: HashSet::new(),
            constructor_used_names: collect_constructor_names(program),
            trait_decls: HashMap::new(),
            type_trait_impls: HashMap::new(),
            traits_needing_interface: HashSet::new(),
            trait_impl_blocks: HashMap::new(),
            local_for_block_fn_names: HashSet::new(),
        };

        // Pre-register union variant info and type defs from imported types.
        for imports in resolved_imports.values() {
            for decl in &imports.type_decls {
                let typed = crate::checker::attach_type_decl_shallow(decl);
                ctx.register_union_variants(&typed);
                ctx.type_defs.insert(typed.name.clone(), typed.def.clone());
            }
        }

        // A local definition wins over a stdlib pipe template, and the
        // checker reads the same set, so the two passes agree on which
        // function a bare name in a pipe calls.
        ctx.local_names = file_scope_names(&program.items);

        // First pass: collect union variant info, traits, etc.
        for item in &program.items {
            match &item.kind {
                ItemKind::TypeDecl(decl) => {
                    ctx.register_union_variants(decl);
                    ctx.type_defs.insert(decl.name.clone(), decl.def.clone());
                }
                ItemKind::Function(decl) => {
                    for tp in &decl.type_params {
                        for bound in &tp.bounds {
                            ctx.traits_needing_interface.insert(bound.clone());
                        }
                    }
                }
                ItemKind::Import(decl) => {
                    if let Some(resolved) = ctx.resolved_imports.get(&decl.source).cloned() {
                        for block in &resolved.for_blocks {
                            ctx.register_for_block_fns(block);
                            if let Some(trait_name) = &block.trait_name
                                && let Some(name) = for_block_base_type_name(&block.type_name)
                            {
                                ctx.type_trait_impls
                                    .entry(name.to_string())
                                    .or_default()
                                    .push(trait_name.clone());
                            }
                        }
                        for decl in &resolved.trait_decls {
                            let typed = crate::checker::attach_trait_decl_shallow(decl);
                            ctx.trait_decls.entry(typed.name.clone()).or_insert(typed);
                        }
                    }
                }
                ItemKind::ForBlock(block) => {
                    ctx.register_for_block_fns(block);
                    for func in &block.functions {
                        ctx.local_for_block_fn_names
                            .insert(for_block_fn_name(&block.type_name, &func.name));
                    }
                    if let Some(trait_name) = &block.trait_name
                        && let Some(name) = for_block_base_type_name(&block.type_name)
                    {
                        ctx.type_trait_impls
                            .entry(name.to_string())
                            .or_default()
                            .push(trait_name.clone());
                        ctx.trait_impl_blocks
                            .entry(name.to_string())
                            .or_default()
                            .push(block.clone());
                    }
                }
                ItemKind::TraitDecl(decl) => {
                    ctx.trait_decls.insert(decl.name.clone(), decl.clone());
                }
                _ => {}
            }
        }

        ctx
    }

    fn register_union_variants(&mut self, decl: &TypedTypeDecl) {
        if let TypeDef::Union(variants) = &decl.def {
            for variant in variants {
                let field_names: Vec<String> = variant
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        f.name.clone().unwrap_or_else(|| {
                            type_layout::positional_field_name(i, variant.fields.len())
                        })
                    })
                    .collect();
                self.variant_info
                    .insert(variant.name.clone(), (decl.name.clone(), field_names));
            }
        }
    }

    pub(super) fn register_for_block_fns<T>(&mut self, block: &ForBlock<T>) {
        let Some(type_name) = for_block_base_type_name(&block.type_name).map(str::to_string) else {
            return;
        };
        self.for_block_type_names.insert(type_name.clone());
        for func in &block.functions {
            let shape = FnShape {
                params: func
                    .params
                    .iter()
                    .map(|p| {
                        if p.name == "self" {
                            return Some(type_name.clone());
                        }

                        p.type_ann.as_ref().and_then(written_annotation_name)
                    })
                    .collect(),
                ret: func.return_type.as_ref().and_then(written_annotation_name),
            };
            let entry = ForBlockFn {
                mangled: for_block_fn_name(&block.type_name, &func.name),
                shape,
            };
            self.for_block_fns
                .insert((type_name.clone(), func.name.clone()), entry.clone());
            self.for_block_fns_by_name
                .entry(func.name.clone())
                .or_default()
                .push(entry);
        }
    }

    /// The for-block function a bare name calls, as the checker resolved it.
    ///
    /// `ty` is the type the checker recorded for the name at this use site.
    /// It picks between two for-blocks that declare one method name, and it
    /// rejects the map's answer when a local binding shadows the name. An
    /// undetermined type carries no evidence either way, so the first
    /// registration answers, which is what an untyped tree needs.
    pub(super) fn resolved_for_block_fn(&self, name: &str, ty: &Type) -> Option<&ForBlockFn> {
        let entries = self.for_block_fns_by_name.get(name)?;
        if ty.is_undetermined() {
            return entries.first();
        }

        entries.iter().find(|entry| {
            checker_agrees(
                ty,
                &GlobalName::ForBlockFn {
                    shape: &entry.shape,
                },
            )
        })
    }

    /// Returns true if the name is used as a for-block type prefix but NOT
    /// as a runtime value (constructor, call, etc).
    pub(super) fn is_for_block_type_only(&self, name: &str) -> bool {
        self.for_block_type_names.contains(name) && !self.constructor_used_names.contains(name)
    }

    /// The annotation a type name stands for, when the name is an alias.
    ///
    /// `typealias Id = string` gives back `string`, and `opaque type Pw =
    /// string` gives back `string` too, because both lower to
    /// `TypeDef::Alias`. A record, a tagged union or a string-literal union
    /// declares its own shape, so the answer is `None`. A chain of aliases
    /// walks to its end, and a cycle answers `None` instead of hanging.
    ///
    /// `parse<T>` and `mock<T>` both read this one function, so the two
    /// built-ins cannot part on what a name means. Before #1521 `mock`
    /// followed the chain through `type_defs` and `parse` read the written
    /// name only, so `parse<Id>` validated an alias of `string` as an
    /// object and rejected every valid string at run time.
    pub(super) fn alias_target(&self, name: &str) -> Option<&TypedTypeExpr> {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut current = name;
        let mut target: Option<&TypedTypeExpr> = None;

        loop {
            if !seen.insert(current) {
                return None;
            }
            let Some(TypeDef::Alias(type_expr)) = self.type_defs.get(current) else {
                return target;
            };
            target = Some(type_expr);
            match &type_expr.kind {
                TypeExprKind::Named {
                    name: next,
                    type_args,
                    ..
                } if type_args.is_empty() => current = next,
                _ => return target,
            }
        }
    }
}

/// Mutable emission state for the TypeScript generator.
pub(crate) struct TypeScriptGenerator<'a> {
    pub(super) ctx: &'a TypeContext,
    pub(super) import_aliases: HashMap<String, String>,
    pub(super) current_type_param_bounds: HashMap<String, Vec<String>>,
    pub(super) needs_deep_equal: bool,
    pub(super) has_jsx: bool,
    pub(super) unwrap_counter: usize,
    /// Types whose `{T}__make` factory has already been emitted, so
    /// subsequent trait-impl for-blocks for the same type don't re-emit it.
    pub(super) emitted_factories: HashSet<String>,
}

impl<'a> TypeScriptGenerator<'a> {
    pub fn new(ctx: &'a TypeContext) -> Self {
        Self {
            ctx,
            import_aliases: HashMap::new(),
            current_type_param_bounds: HashMap::new(),
            needs_deep_equal: false,
            has_jsx: false,
            unwrap_counter: 0,
            emitted_factories: HashSet::new(),
        }
    }

    /// Generate TypeScript from a typed Floe program.
    pub fn generate(&mut self, program: &TypedProgram) -> CodegenOutput {
        // Emit TypeScript interfaces for all traits used as generic bounds
        let interface_doc = self.emit_trait_interfaces();
        let has_interfaces = !matches!(&interface_doc, Document::Vec(v) if v.is_empty());

        let mut docs: Vec<Document> = Vec::new();

        if has_interfaces {
            docs.push(interface_doc);
        }

        for (i, item) in program.items.iter().enumerate() {
            if i > 0 || has_interfaces {
                docs.push(pretty::str("\n"));
            }
            docs.push(self.emit_item(item));
            docs.push(pretty::str("\n"));
        }

        let main_doc = pretty::concat(docs);

        // Prepend structural equality helper if any == or != was used
        let final_doc = if self.needs_deep_equal {
            pretty::concat([deep_equal_doc(), main_doc])
        } else {
            main_doc
        };

        let mut code = String::new();
        final_doc
            .pretty_print_to(PRINT_WIDTH, &mut code)
            .expect("String as fmt::Write never fails");
        let dts = self.generate_dts(program);

        CodegenOutput {
            code,
            has_jsx: self.has_jsx,
            dts,
        }
    }

    /// Render a Document to a String (for embedding in format strings, templates, etc.).
    pub(super) fn doc_to_string(doc: &Document) -> String {
        let mut out = String::new();
        doc.pretty_print_to(PRINT_WIDTH, &mut out)
            .expect("String as fmt::Write never fails");
        out
    }
}

/// The deep-equality helper function, prepended when `==` or `!=` is used.
fn deep_equal_doc() -> Document {
    pretty::str(format!(
        "function {DEEP_EQUAL_FN}(a: unknown, b: unknown): boolean {{\n\
         \x20\x20if (a === b) return true;\n\
         \x20\x20if (a == null || b == null) return false;\n\
         \x20\x20if (typeof a !== \"object\" || typeof b !== \"object\") return false;\n\
         \x20\x20const ka = Object.keys(a as object);\n\
         \x20\x20const kb = Object.keys(b as object);\n\
         \x20\x20if (ka.length !== kb.length) return false;\n\
         \x20\x20return ka.every((k) => {DEEP_EQUAL_FN}((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));\n\
         }}\n\n"
    ))
}
