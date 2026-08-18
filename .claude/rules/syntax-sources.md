# Syntax Highlighting Sources of Truth

When changing Floe syntax (new keywords, operators, or language features), you MUST update ALL of these:

1. **Tree-sitter grammar** — `editors/tree-sitter-floe/grammar.js` + `queries/highlights.scm`
   - Run `tree-sitter generate && tree-sitter test` after changes
2. **TextMate grammar** (VSCode) — `editors/vscode/syntaxes/floe.tmLanguage.json`
3. **VSCode snippets** — `editors/vscode/snippets/floe.json`
4. **VSCode language configuration** — `editors/vscode/language-configuration.json`
   - Holds `wordPattern`, which decides what a double click selects and what
     word-based navigation treats as one word. Change it when the identifier
     rule changes.
5. **Neovim queries** — `editors/neovim/queries/floe/highlights.scm` (copy from tree-sitter)
6. **Site docs** — `docs/site/src/content/docs/`
7. **LLM reference** — `docs/llms.txt`

Also update test fixtures, example code, and README if affected.

There is NO Vim syntax file — Neovim uses tree-sitter directly.
