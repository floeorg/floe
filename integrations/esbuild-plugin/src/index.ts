import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import type { Plugin } from "esbuild";

export interface FloeOptions {
  /** Path to the `floe` binary. Defaults to `"floe"`. */
  compiler?: string;
}

/**
 * esbuild plugin for Floe.
 *
 * Resolves extensionless relative imports to `.fl` sources and serves the
 * compiled TypeScript from the project's `.floe/` mirror when it exists,
 * falling back to an on-demand `floe build` invocation otherwise.
 *
 * Works under wrangler, Bun's bundler, tsup, electron-forge, and raw esbuild —
 * any bundler that accepts an esbuild plugin.
 *
 * @example
 * ```ts
 * import floe from "@floeorg/esbuild-plugin";
 * import { build } from "esbuild";
 *
 * await build({
 *   entryPoints: ["src/app.ts"],
 *   bundle: true,
 *   plugins: [floe()],
 * });
 * ```
 */
export default function floe(options: FloeOptions = {}): Plugin {
  const compiler = options.compiler ?? "floe";

  return {
    name: "floe",
    setup(build) {
      build.onResolve({ filter: /^\.\.?\// }, (args) => {
        const basePath = resolve(args.resolveDir, args.path);

        // If the import explicitly references a .fl file, take it.
        if (basePath.endsWith(".fl") && existsSync(basePath)) {
          return { path: basePath };
        }

        // For extensionless imports, prefer `.fl` when nothing with a
        // conventional extension exists at the target — Floe-native
        // modules don't have `.ts` siblings in `src/`.
        if (!basePath.endsWith(".fl")) {
          for (const ext of [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"]) {
            if (existsSync(basePath + ext)) return;
          }
          const flPath = basePath + ".fl";
          if (existsSync(flPath)) {
            return { path: flPath };
          }
        }

        return;
      });

      build.onLoad({ filter: /\.fl$/ }, (args) => {
        const cached = readCachedOutput(args.path);
        if (cached !== null) {
          return { contents: cached, loader: "ts", resolveDir: dirname(args.path) };
        }

        try {
          const compiled = compileFloe(compiler, args.path);
          return { contents: compiled, loader: "ts", resolveDir: dirname(args.path) };
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          return {
            errors: [
              {
                text: `Floe compilation failed for ${args.path}`,
                detail: message,
              },
            ],
          };
        }
      });
    },
  };
}

/**
 * Read pre-compiled `.ts`/`.tsx` output from the `.floe/` mirror if it's
 * fresher than the source `.fl`. Returns null if the mirror is missing,
 * stale, or the project root can't be located.
 */
function readCachedOutput(flFile: string): string | null {
  const projectRoot = findProjectRoot(flFile);
  if (!projectRoot) return null;

  let sourceMtime: number;
  try {
    sourceMtime = statSync(flFile).mtimeMs;
  } catch {
    return null;
  }

  const rel = relative(projectRoot, flFile);
  const floeDir = join(projectRoot, ".floe");

  for (const ext of ["tsx", "ts"]) {
    const outPath = join(floeDir, rel).replace(/\.fl$/, `.${ext}`);
    try {
      if (statSync(outPath).mtimeMs >= sourceMtime) {
        return readFileSync(outPath, "utf-8");
      }
    } catch {
      // try next extension
    }
  }

  return null;
}

/** Walk up from `start` looking for a directory that contains `package.json`. */
function findProjectRoot(start: string): string | null {
  let dir = isAbsolute(start) ? dirname(start) : resolve(start);
  while (true) {
    if (existsSync(join(dir, "package.json"))) return dir;
    const parent = dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

function compileFloe(compiler: string, filename: string): string {
  try {
    return execFileSync(compiler, ["build", "--emit-stdout", filename], {
      encoding: "utf-8",
      timeout: 30_000,
      stdio: ["pipe", "pipe", "pipe"],
    });
  } catch (error) {
    if (error && typeof error === "object" && "stderr" in error) {
      const stderr = (error as { stderr: string | Buffer }).stderr;
      throw new Error(String(stderr));
    }
    throw error;
  }
}
