import { compileFloe, findProjectRoot, readCompiledOutput } from "@floeorg/core";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import type { Plugin } from "esbuild";

export interface FloeOptions {
  /** Path to the `floe` binary. Defaults to `"floe"`. */
  compiler?: string;
}

const JS_TS_EXTENSIONS = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"] as const;

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
      const rootCache = new Map<string, string>();
      const findRoot = (flFile: string): string => {
        const key = dirname(flFile);
        const cached = rootCache.get(key);
        if (cached !== undefined) return cached;
        const found = findProjectRoot(key);
        rootCache.set(key, found);
        return found;
      };

      build.onResolve({ filter: /^\.\.?\// }, (args) => {
        const basePath = resolve(args.resolveDir, args.path);

        if (basePath.endsWith(".fl") && existsSync(basePath)) {
          return { path: basePath };
        }

        // Floe-native modules don't have `.ts` siblings — prefer `.fl` only
        // when no conventional extension matches, so mixed projects with
        // handwritten `.ts` keep working.
        if (!basePath.endsWith(".fl")) {
          for (const ext of JS_TS_EXTENSIONS) {
            if (existsSync(basePath + ext)) return;
          }
          const flPath = basePath + ".fl";
          if (existsSync(flPath)) return { path: flPath };
        }
      });

      build.onLoad({ filter: /\.fl$/ }, (args) => {
        const resolveDir = dirname(args.path);
        const projectRoot = findRoot(args.path);
        const cached = readCompiledOutput(args.path, projectRoot);
        if (cached !== null) {
          return { contents: cached, loader: "ts", resolveDir };
        }

        try {
          const { code } = compileFloe(compiler, args.path);
          return { contents: code, loader: "ts", resolveDir };
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
