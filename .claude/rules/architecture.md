# Compiler Architecture

## Pipeline

```
Source → Lexer → CST → Lower → AST → Checker → Annotate → Desugar → Codegen → TypeScript
```

### Stages

| Stage | File(s) | Input | Output |
|---|---|---|---|
| Lex + Parse | `lexer.rs`, `parser.rs` | source string | CST (concrete syntax tree) |
| Lower | `lower.rs`, `lower/expr.rs` | CST | AST with `ExprId` on every `Expr`, types set to `Unknown` |
| Check | `checker.rs`, `checker/expr.rs` | `&Program` | diagnostics + `ExprTypeMap` (HashMap<ExprId, Type>) |
| Annotate | `checker::annotate_types()` | `&mut Program` + `ExprTypeMap` | every `Expr.ty` filled with its resolved `Type` |
| Desugar | `desugar.rs` | `&mut Program` (typed) | transformed AST (Some/None eliminated) |
| Codegen | `codegen.rs`, `codegen/expr.rs` | `&Program` (typed + desugared) | TypeScript source |

### Key design: typed AST

Every `Expr` carries its resolved type:

```rust
pub struct Expr {
    pub id: ExprId,
    pub kind: ExprKind,
    pub ty: Type,       // Unknown before checking, resolved after
    pub span: Span,
}
```

After the checker runs, `annotate_types()` walks the AST and fills in `expr.ty` from the `ExprTypeMap`. From that point on, every pass (desugar, codegen) can read types directly from expression nodes - no separate type map lookup needed.

### Adding a new language feature

1. **Lexer**: add token(s) in `lexer/token.rs`
2. **Parser**: add CST node(s) in `syntax.rs`, parse in `parser.rs`
3. **Lower**: add `ExprKind` variant in `parser/ast.rs`, lower from CST in `lower/expr.rs`
4. **Checker**: type-check in `checker/expr.rs` - the resolved type is stored via `check_expr` → `expr_types.insert(expr.id, ty)`
5. **Desugar** (if needed): transform to simpler AST nodes in `desugar.rs` - can read `expr.ty` for type-directed transforms
6. **Codegen**: emit TypeScript in `codegen/expr.rs` - can read `expr.ty` for type-directed emission

### What each pass should NOT do (target state)

- **Checker** should not emit code or transform the AST
- **Desugar** should not produce diagnostics or do type checking
- **Codegen** should not carry semantic state - read types from `expr.ty` instead. Note: codegen still carries `StdlibRegistry` for pipe template expansion until pipe desugaring is implemented.

### Key modules

| Module | Purpose |
|---|---|
| `type_layout.rs` | Runtime type representation - field names, variant discriminants, field accessors, type-to-stdlib-module mapping |
| `stdlib.rs` | Standard library function registry - type signatures and codegen templates |
| `resolve.rs` | Import resolution for .fl files |
| `interop/` | npm/.d.ts import resolution via tsgo |
| `desugar.rs` | AST transforms between checker and codegen |
