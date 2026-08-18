import { compileFloe, readCompiledOutput, type FloeOptions } from "@floeorg/core";
import * as vite from "vite";
import type { Rollup } from "vite";

export type { FloeOptions };

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

    // `.fl` joins `resolve.extensions` so that `import { Page } from "./page"`
    // finds `page.fl`. It must NOT join Vite's own transform filter
    // (`esbuild.include` on Vite 5 to 7, `oxc.include` on Vite 8): the
    // `transform` hook below already returns plain JavaScript, and Vite 8 runs
    // that filter inside rolldown, whose `builtin:vite-transform` reads the
    // language from the file extension alone and fails with "Failed to detect
    // the lang of <file>.fl".
    //
    // Replacing the filter also repairs `.mts`, which the old entry dropped:
    // `/\.(tsx?|jsx?|fl)$/` does not match it and Vite's default
    // `/\.(m?ts|[jt]sx)$/` does.
    config(config: { resolve?: { extensions?: string[] } }) {
      const existing = config.resolve?.extensions ?? [".mjs", ".js", ".mts", ".ts", ".jsx", ".tsx", ".json"];
      const extensions = existing.includes(".fl") ? existing : [...existing, ".fl"];

      return {
        resolve: { extensions },
      };
    },

    async transform(this: { error(msg: string): never }, code: string, id: string) {
      // Strip query params for extension check (Vite adds ?import, ?t=xxx, etc.)
      const cleanId = id.split("?")[0];
      if (!cleanId.endsWith(".fl")) return null;

      try {
        // .floe/ output is kept fresh by `floe watch`.
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

/** What the `transform` hook hands back to the bundler. */
interface FloeTransformResult {
  readonly code: string;
  readonly map: Rollup.SourceMapInput | undefined;
  /**
   * The language of `code`, stated rather than guessed.
   *
   * `moduleType` is part of rolldown's public plugin API
   * (`SourceDescription`), and Vite's own oxc plugin returns `"js"` from its
   * transform hook for the same reason. Without it rolldown reads the module
   * type from the `.fl` extension, finds nothing, and falls back to JavaScript.
   * That fallback is real but undocumented: rolldown's module-types page never
   * states it, and the only statement of the rule is a comment in
   * `crates/rolldown/src/utils/load_source.rs`. A rolldown patch release could
   * drop it and kill every `.fl` build, so say the answer here instead.
   *
   * Rollup ignores the field, so Vite 5 to 7 are unaffected.
   */
  readonly moduleType: "js";
}

// Vite 6+ has transformWithOxc, Vite 5 has transformWithEsbuild.
// Use whichever is available for cross-version compatibility.
async function transformTsx(code: string, id: string): Promise<FloeTransformResult> {
  const filename = id + ".tsx";
  const transformed = await runTransform(code, filename);

  return { code: transformed.code, map: transformed.map, moduleType: "js" };
}

function runTransform(
  code: string,
  filename: string,
): Promise<{ code: string; map: Rollup.SourceMapInput | undefined }> {
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

