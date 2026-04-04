---
title: Configuration
---

## tsconfig.json

Floe outputs TypeScript files, so your project needs a `tsconfig.json`. The `floe init` command creates one for you:

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
- `jsx: "react-jsx"` - required for `.tsx` output from Floe JSX
- `strict: true` - matches Floe's strictness philosophy
- `moduleResolution: "bundler"` - works with Vite and modern bundlers

## Project Structure

Recommended layout:

```
my-app/
  src/
    main.fl           # Entry point
    components/
      App.fl           # React components
      Button.fl
    utils/
      math.fl          # Utility functions
  tsconfig.json
  package.json
  vite.config.ts       # If using Vite
```

## Build Output

By default, `floe build` and `floe watch` output compiled files to a `.floe/` directory at the project root, mirroring the source tree:

```
src/main.fl    -> .floe/src/main.ts
src/App.fl     -> .floe/src/App.tsx    (if JSX detected)
```

The `.floe/` directory also contains `.d.fl.ts` type declarations so TypeScript can resolve `.fl` imports. Add `rootDirs` to your `tsconfig.json` to make this transparent:

```json
{
  "compilerOptions": {
    "allowArbitraryExtensions": true,
    "rootDirs": ["./src", "./.floe/src"]
  }
}
```

Add `.floe/` to your `.gitignore` -- it's a build artifact.

Use `--out-dir` to specify a different output directory:

```bash
floe build src/ --out-dir dist/
```

## package.json Scripts

```json
{
  "scripts": {
    "dev": "floe watch src/",
    "build": "floe build src/",
    "check": "floe check src/"
  }
}
```

Run `floe watch` alongside your dev server. Since `floe watch` writes standard `.ts`/`.tsx` files to `.floe/`, any tool that handles TypeScript works automatically -- Vite, wrangler, node, bun, esbuild, webpack, etc.

## npm Interop

Floe resolves npm modules using your project's `tsconfig.json` and `node_modules`. No additional configuration is needed.

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
