import { compileFloe, readCompiledOutput } from "@floeorg/core";
import * as vite from "vite";

export interface FloeOptions {
  /** Path to the floe binary. Defaults to "floe". */
  compiler?: string;
}

/**
 * Vite plugin for Floe.
 *
 * Transforms `.fl` files to TypeScript in the build pipeline.
 * Reads pre-compiled output from `.floe/` when available (kept
 * up-to-date by `floe watch`), falling back to on-demand compilation
 * via the `floe` CLI.
 *
 * @example
 * ```ts
 * import { defineConfig } from "vite"
 * import floe from "@floeorg/vite-plugin"
 *
 * export default defineConfig({
 *   plugins: [floe()],
 * })
 * ```
 */
export default function floe(options: FloeOptions = {}): import("vite").Plugin {
  const compiler = options.compiler ?? "floe";
  let projectRoot: string;

  return {
    name: "@floeorg/vite-plugin",
    enforce: "pre" as const,

    configResolved(config: { root: string }) {
      projectRoot = config.root;
    },

    config(config: { resolve?: { extensions?: string[] } }) {
      const existing = config.resolve?.extensions ?? [".mjs", ".js", ".mts", ".ts", ".jsx", ".tsx", ".json"];
      const extensions = existing.includes(".fl") ? existing : [...existing, ".fl"];
      return {
        resolve: { extensions },
        esbuild: {
          include: /\.(tsx?|jsx?|fl)$/,
          loader: "tsx" as const,
        },
      };
    },

    async transform(this: { error(msg: string): never }, code: string, id: string) {
      // Strip query params for extension check (Vite adds ?import, ?t=xxx, etc.)
      const cleanId = id.split("?")[0];
      if (!cleanId.endsWith(".fl")) return null;

      try {
        // Try reading pre-compiled output from .floe/ (kept fresh by `floe watch`)
        const cached = readCompiledOutput(cleanId, projectRoot);
        if (cached) {
          return transformTsx(cached, cleanId);
        }

        const compiled = compileFloe(compiler, id);
        return transformTsx(compiled.code, cleanId);
      } catch (error) {
        const message =
          error instanceof Error ? error.message : String(error);
        this.error(`Floe compilation failed for ${id}:\n${message}`);
      }
    },

    handleHotUpdate({ file, server }: { file: string; server: { moduleGraph: { getModulesByFile(file: string): Set<any> | undefined } } }) {
      if (file.endsWith(".fl")) {
        const modules = server.moduleGraph.getModulesByFile(file);
        if (modules) {
          return [...modules];
        }
      }
    },
  };
}

// Vite 6+ has transformWithOxc, Vite 5 has transformWithEsbuild.
// Use whichever is available for cross-version compatibility.
function transformTsx(code: string, id: string) {
  const filename = id + ".tsx";

  if ("transformWithOxc" in vite) {
    return (vite as any).transformWithOxc(code, filename, {
      lang: "tsx",
      jsx: { runtime: "automatic" },
    });
  }

  if ("transformWithEsbuild" in vite) {
    return (vite as any).transformWithEsbuild(code, filename, {
      jsx: "automatic",
      loader: "tsx",
    });
  }

  throw new Error(
    "Floe vite plugin: neither transformWithOxc nor transformWithEsbuild found. " +
    "Please use Vite 5 or later.",
  );
}

