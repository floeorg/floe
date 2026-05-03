use std::collections::{HashMap, HashSet};

use crate::parser::ast::{
    ConstBinding, ForBlock, ItemKind, TypeDef, TypedForBlock, TypedProgram, TypedTraitDecl,
    TypedTypeDecl, TypedTypeDef,
};
use crate::pretty::{self, Document};
use crate::resolve::ResolvedImports;
use crate::stdlib::StdlibRegistry;
use crate::type_layout;

use super::super::{
    CodegenOutput, DEEP_EQUAL_FN, collect_constructor_names, collect_value_used_names,
    for_block_base_type_name, for_block_fn_name,
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

/// Read-only type metadata collected during the first pass.
/// Borrowed by the generator — no cloning needed for sub-expressions.
pub(crate) struct TypeContext {
    pub stdlib: StdlibRegistry,
    pub unit_variants: HashSet<String>,
    pub variant_info: HashMap<String, (String, Vec<String>)>,
    pub type_defs: HashMap<String, TypedTypeDef>,
    pub local_names: HashSet<String>,
    pub resolved_imports: HashMap<String, ResolvedImports>,
    pub test_mode: bool,
    pub value_used_names: HashSet<String>,
    pub for_block_fns: HashMap<(String, String), String>,
    /// Bare-name index into `for_block_fns`: maps `fn_name` → mangled name.
    /// Lets pipe/identifier lookup hit a HashMap instead of scanning.
    pub for_block_fns_by_name: HashMap<String, String>,
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
            unit_variants: HashSet::new(),
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
        };

        // Pre-register union variant info and type defs from imported types.
        for imports in resolved_imports.values() {
            for decl in &imports.type_decls {
                let typed = crate::checker::attach_type_decl_shallow(decl);
                ctx.register_union_variants(&typed);
                ctx.type_defs.insert(typed.name.clone(), typed.def.clone());
            }
        }

        // First pass: collect union variant info, local names, traits, etc.
        for item in &program.items {
            match &item.kind {
                ItemKind::TypeDecl(decl) => {
                    ctx.register_union_variants(decl);
                    ctx.type_defs.insert(decl.name.clone(), decl.def.clone());
                }
                ItemKind::Function(decl) => {
                    ctx.local_names.insert(decl.name.clone());
                    for tp in &decl.type_params {
                        for bound in &tp.bounds {
                            ctx.traits_needing_interface.insert(bound.clone());
                        }
                    }
                }
                ItemKind::Const(decl) => {
                    if let ConstBinding::Name(name) = &decl.binding {
                        ctx.local_names.insert(name.clone());
                    }
                }
                ItemKind::Import(decl) => {
                    for spec in &decl.specifiers {
                        let name = spec.alias.as_ref().unwrap_or(&spec.name);
                        ctx.local_names.insert(name.clone());
                    }
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
                        ctx.local_names.insert(func.name.clone());
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
                if variant.fields.is_empty() {
                    self.unit_variants.insert(variant.name.clone());
                }
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
            let mangled = for_block_fn_name(&block.type_name, &func.name);
            self.for_block_fns
                .insert((type_name.clone(), func.name.clone()), mangled.clone());
            // First registration wins for the bare-name lookup. Method names
            // are unique across for-blocks in practice because codegen would
            // emit ambiguous calls otherwise.
            self.for_block_fns_by_name
                .entry(func.name.clone())
                .or_insert(mangled);
        }
    }

    /// Look up a for-block function by bare name (without type qualifier).
    pub(super) fn lookup_for_block_fn_by_name(
        &self,
        name: &str,
        import_aliases: &HashMap<String, String>,
    ) -> Option<String> {
        let mangled = self.for_block_fns_by_name.get(name)?;
        Some(
            import_aliases
                .get(mangled)
                .cloned()
                .unwrap_or_else(|| mangled.clone()),
        )
    }

    /// Returns true if the name is used as a for-block type prefix but NOT
    /// as a runtime value (constructor, call, etc).
    pub(super) fn is_for_block_type_only(&self, name: &str) -> bool {
        self.for_block_type_names.contains(name) && !self.constructor_used_names.contains(name)
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
