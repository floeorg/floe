<!-- managed by milky-kit | DO NOT EDIT — changes will be overwritten on next sync -->

# General Practices

- **No abbreviations in communication.** Write full words in responses. Avoid abbreviations like "cfg" (config), "deps" (dependencies), "repo" (repository), etc. Common acronyms like CI, DI, PR are fine.

- **Use libraries.** Search for existing crates/packages before building. Prefer mature, well-maintained ones. Only roll your own when nothing suitable exists or the functionality is trivial.
- **Use CLI to add packages** — never edit manifest files by hand. Rust: `cargo add <crate>` from the app directory. Node: `pnpm add <package>` from the app directory.
- **Use Context7 MCP** for library/framework docs before guessing APIs. Use WebSearch/WebFetch when stuck.
- **If something is hard**, don't skip it. Search the internet. If still stuck, leave a stub and a TODO comment.
- **No bandaid fixes.** Always do the correct, elegant, and scalable fix. If a quick hack would fix the symptom but leave the root cause or create tech debt, stop and think about the proper solution first. Ask the user if unsure. A correct fix now saves three fix-the-fix PRs later.
- **Bug fixes start with a failing test.** Reproduce the bug with a test that fails, then fix it.
- **No backwards compatibility.** When changing or removing code, delete it completely. No deprecated wrappers, re-exports, shims, or `// removed` comments. If it's unused, it's gone.
- **Always format before pushing.** Run `cargo fmt` (Rust) and `pnpm lint:fix` (frontend) before every `git push`. No exceptions.
- **Merge, don't rebase.** Use `git merge main` to update branches, not `git rebase`. Rebase rewrites history and requires force push, which can destroy work in multi-agent environments.
- **Never force push.** No `git push --force`, `git push -f`, or `--force-with-lease`.
