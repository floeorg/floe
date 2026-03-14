---
title: Configuration
---

## tsconfig.json

ZenScript outputs TypeScript files, so your project needs a `tsconfig.json`. The `zsc init` command creates one for you:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "skipLibCheck": true
  },
  "include": ["src/**/*.ts", "src/**/*.tsx"]
}
```

Key settings:
- `jsx: "react-jsx"` — required for `.tsx` output from ZenScript JSX
- `strict: true` — matches ZenScript's strictness philosophy
- `moduleResolution: "bundler"` — works with Vite and modern bundlers

## Project Structure

Recommended layout:

```
my-app/
  src/
    main.zs           # Entry point
    components/
      App.zs           # React components
      Button.zs
    utils/
      math.zs          # Utility functions
  tsconfig.json
  package.json
  vite.config.ts       # If using Vite
```

## Build Output

By default, `zsc build` outputs files next to the source:

```
src/main.zs    -> src/main.ts
src/App.zs     -> src/App.tsx    (if JSX detected)
```

Use `--out-dir` to specify a separate output directory:

```bash
zsc build src/ --out-dir dist/
```

## npm Interop

ZenScript resolves npm modules using your project's `tsconfig.json` and `node_modules`. No additional configuration is needed.

When importing from npm packages:
- `T | null` becomes `Option<T>`
- `T | undefined` becomes `Option<T>`
- `any` becomes `unknown`

This happens automatically at the import boundary.

## Ignoring Directories

The compiler automatically skips:
- `node_modules/`
- Hidden directories (`.git`, `.vscode`, etc.)
- `target/` (Rust build output)
