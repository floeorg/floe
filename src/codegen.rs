mod dts;
mod expr;
mod items;
mod jsx;
mod match_emit;
mod parse_mock;
mod pipes;
#[cfg(test)]
mod tests;
mod types;

use std::collections::{HashMap, HashSet};

use crate::parser::ast::*;
use crate::resolve::ResolvedImports;
use crate::stdlib::StdlibRegistry;
use crate::type_layout;
use crate::type_layout::{ERROR_FIELD, OK_FIELD, TAG_FIELD, VALUE_FIELD};

// ── Runtime codegen constants ───────────────────────────────────

/// Runtime deep-equality helper function name.
const DEEP_EQUAL_FN: &str = "__floeEq";

/// `todo` expression — throws "not implemented" at runtime.
const THROW_NOT_IMPLEMENTED: &str = "(() => { throw new Error(\"not implemented\"); })()";

/// `unreachable` expression — throws "unreachable" at runtime.
const THROW_UNREACHABLE: &str = "(() => { throw new Error(\"unreachable\"); })()";

/// Fallback for non-exhaustive match — throws at runtime.
const THROW_NON_EXHAUSTIVE: &str = "(() => { throw new Error(\"non-exhaustive match\"); })()";

/// Mock placeholder for function types — throws when called.
const THROW_MOCK_FUNCTION: &str = "(() => { throw new Error(\"mock function\"); })";

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

    /// Look up a for-block function by bare name (without type qualifier).
    /// Returns the mangled name if found (e.g., "toChar" → "Icon__toChar").
    fn lookup_for_block_fn_by_name(&self, name: &str) -> Option<String> {
        for ((_, fn_name), mangled) in &self.for_block_fns {
            if fn_name == name {
                return Some(
                    self.import_aliases
                        .get(mangled)
                        .cloned()
                        .unwrap_or_else(|| mangled.clone()),
                );
            }
        }
        None
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

    // ── Output helpers ───────────────────────────────────────────

    pub(super) fn push(&mut self, s: &str) {
        self.output.push_str(s);
    }

    pub(super) fn newline(&mut self) {
        self.output.push('\n');
    }

    pub(super) fn emit_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
    }

    pub(super) fn expr_to_string(&self, expr: &Expr) -> String {
        let mut cg = self.sub_codegen();
        cg.emit_expr(expr);
        cg.output
    }

    /// Create a sub-codegen that shares type info but has its own output buffer.
    pub(super) fn sub_codegen(&self) -> Codegen {
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
        ExprKind::Try(e) | ExprKind::Unwrap(e) => {
            collect_value_names_from_expr(e, names);
        }
        ExprKind::Value(e) => {
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
