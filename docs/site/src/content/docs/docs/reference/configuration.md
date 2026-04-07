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

### `lib` and `types` - controlling globals

Floe reads `compilerOptions.lib` and `compilerOptions.types` to determine which globals are available:

```json
// Browser project (default if lib is omitted)
{ "compilerOptions": { "lib": ["ES2020", "DOM"] } }
// → window, document, navigator, fetch, Date, Promise, etc.

// Node.js backend
{ "compilerOptions": { "lib": ["ES2020"], "types": ["node"] } }
// → process, Buffer, __dirname (no window/document)

// Cloudflare Workers
{ "compilerOptions": { "lib": ["ESNext"], "types": ["@cloudflare/workers-types"] } }
// → Workers globals, Date, Promise (no window/document)
```

- **`lib`** controls which TypeScript lib files are loaded (ES versions, DOM, WebWorker, etc.)
- **`types`** controls which `@types/*` packages are loaded for global declarations
- If `types` is omitted, all installed `@types/*` packages are auto-included (TypeScript default)
- If `lib` is omitted, defaults to `ES5` + `DOM`

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

For **Vite** projects (using `@floeorg/vite-plugin`):

```json
{
  "scripts": {
    "dev": "vite",
    "build": "floe build src/ && vite build",
    "check": "floe check src/"
  }
}
```

For **Node / backend** projects (using `@floeorg/register`):

```json
{
  "scripts": {
    "dev": "floe watch src/ & node --import @floeorg/register src/app.ts",
    "build": "floe build src/",
    "check": "floe check src/"
  }
}
```

## Integrations

| Package | Runtime | What it does |
|---|---|---|
| [`@floeorg/vite-plugin`](https://www.npmjs.com/package/@floeorg/vite-plugin) | Vite | Transforms `.fl` files in the Vite pipeline with HMR |
| [`@floeorg/register`](https://www.npmjs.com/package/@floeorg/register) | Node, tsx | Resolves `.fl` imports via `.floe/` at runtime |

Both read pre-compiled output from `.floe/` (populated by `floe watch` or `floe build`). The Vite plugin also falls back to on-demand compilation if `.floe/` is missing.

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
