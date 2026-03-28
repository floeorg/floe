# Floe — Design Decisions

For language syntax and usage, see `docs/site/` (user-facing) and `docs/llms.txt` (LLM reference).
For compiler architecture, see `.claude/rules/architecture.md`.

---

## Resolved Design Decisions

| Question | Decision | Rationale |
|----------|----------|-----------|
| Syntax style | TS keywords + Gleam match/pipe | Familiar to React devs, 30min learning curve |
| Function style | `fn` for named, `(x) => expr` for inline closures, `.field` for shorthand | One keyword, two closure forms, no overlap |
| Arrow `->` | Match arms, return types | "Maps to" for control flow and declarations |
| Fat arrow `=>` | Function types | `(T) => U` mirrors TypeScript's function type syntax |
| `const name = (x) => ...` | Compile error | If it has a name, use `fn`. No two ways to name a function. |
| Dot shorthand | `.field` in callback position creates implicit closure | Covers 80% of inline callbacks (filter, map, sort) |
| Qualified variants | `Type.Variant` when ambiguous, bare when unambiguous | Compiler errors on ambiguous bare variants with helpful suggestion |
| Pipe semantics | First-arg default, `_` placeholder | Gleam approach — clean 90% of the time |
| Partial application | `f(a, _)` creates `(x) => f(a, x)` | Free bonus from `_` placeholder |
| Result unwrap | `?` operator (Rust-style) | Cleaner than `use x <- f()`, less new syntax |
| Null handling | No null/undefined, `Option<T>` only | Gleam approach — one concept for "might not exist" |
| npm null interop | Auto-wrap to `Option` at boundary | Transparent — devs never see null |
| Boolean operators | Keep `||`, `&&`, `!` | Everyone knows them, no coercion issues |
| Compiler target | Pure vanilla .tsx, zero dependencies | Eject-friendly, no runtime cost |
| Type keyword | `type` for everything, no `enum` | `enum` is broken in TS; `type` with `|` covers unions, records, brands, opaques |
| Nested unions | Unions can contain other union types, match at any depth | More powerful than Gleam and TS; compiler generates discrimination tags |
| Constructors | `Type(field: value)` — parens, not braces | Same syntax for records, unions, and function calls. Consistent. |
| Record updates | `Type(..existing, field: newValue)` | Gleam-style spread with `..` — compiles to `{ ...existing, field: newValue }` |
| Named arguments | Optional labels at call site: `f(name: "x")` | Self-documenting, order-independent when labelled |
| Default values | `field: Type = default` on types and functions | Required for React DX (component props). Constants only, named-arg-only, required fields first. |
| Object equality | Structural (deep) equality by default | No `class`, all `const` — reference equality is meaningless |
| Unit type | `()` replaces `void` | Real value, works in generics like `Result<(), E>` |
| Array sort | Returns new array, numeric default | No mutation footgun, no lexicographic surprise |
| Numeric parsing | `Number.parse` returns `Result` | No silent `NaN`, no partial parse, no octal weirdness |
| Iteration | Own values only, no prototype chain | `for...in` prototype leakage is eliminated |
| Implicit return | Last expression in a block is the return value; no `return` keyword | No silent `undefined` returns, less noise |
| Spread overlap | Warning on statically-known key overlap | Catches silent overwrites at compile time |
| Compiler language | Rust | Fast, WASM-ready for browser playground, good LSP story |
| Inline tests | `test "name" { assert expr }` co-located with code | Gleam/Rust-inspired; type-checked always, stripped from production output |
| Type definitions | `type Foo { fields }` for records, `type Foo { \| A \| B }` for unions | Unified syntax: all nominal types use `type Name { ... }`. `=` only for aliases and string literal unions |
| For blocks | `for Type { fn f(self) ... }` groups functions under a type | Rust/Swift-like method chaining DX without OOP. `self` is explicit, no `this` magic |

---

## Key Insight

The entire value proposition is: **all the checking happens at compile time, and the output is the simplest possible TypeScript.** There is no runtime. There is no framework. There is no dependency. Just a compiler that turns nice syntax into boring, correct code.

If you eject from Floe, you have normal TypeScript. That's the escape hatch, and it's the most reassuring one possible.
