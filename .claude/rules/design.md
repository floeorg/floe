# Design Specification

The language specification and compiler architecture live in `docs/design.md`.

**Before starting any issue**, read the relevant section of this file. It defines:

- Syntax for every language feature (pipes, match, ?, constructors, etc.)
- What each feature compiles to in TypeScript
- What's banned and why
- AST node types
- Type checker rules
- Compiler strictness rules
- npm interop strategy
- LSP behavior

Issues are intentionally brief. The design doc is the source of truth for how things should work.

## Quick reference - design doc sections

| Working on | Read section |
|---|---|
| Lexer | "Lexer (`floe_lexer`)" - token table + banned tokens |
| Parser | "Parser (`floe_parser`)" - AST node types |
| Codegen | "Code Generator (`floe_codegen`)" - transformation table |
| Type checker | "Type Checker (`floe_checker`)" - 10 rules |
| Syntax questions | "Syntax Design" + "Syntax Examples" |
| Strictness rules | "Compiler Strictness Rules" table |
| npm interop | "npm / .d.ts Interop" |
| LSP | "Language Server (`floe_lsp`)" |
| Any feature | "Resolved Design Decisions" for rationale |
