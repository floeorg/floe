import { execFileSync } from "node:child_process";
import * as vite from "vite";

export interface FloeOptions {
  /** Path to the floe binary. Defaults to "floe". */
  compiler?: string;
}

/**
 * Vite plugin for Floe.
 *
 * Transforms `.fl` files to TypeScript in the build pipeline.
 * Uses the `floe` compiler binary for compilation.
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

  return {
    name: "vite-plugin-floe",
    enforce: "pre" as const,

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
        const compiled = compileFloe(compiler, code, id);

        // The Floe compiler outputs TSX. Transform it to plain JS so
        // Vite's import analysis (es-module-lexer) can parse it.
        // Without this, .fl files keep their original extension in the
        // pipeline and the react plugin skips them (.tsx/.jsx only).
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

interface CompileResult {
  code: string;
  map: string | null;
}

function compileFloe(
  compiler: string,
  _source: string,
  filename: string,
): CompileResult {
  try {
    const output = execFileSync(compiler, ["build", "--emit-stdout", filename], {
      encoding: "utf-8",
      timeout: 30_000,
      stdio: ["pipe", "pipe", "pipe"], // capture stderr instead of printing
    });

    return {
      code: output,
      map: null,
    };
  } catch (error) {
    if (error && typeof error === "object" && "stderr" in error) {
      const stderr = (error as { stderr: string | Buffer }).stderr;
      throw new Error(String(stderr));
    }
    throw error;
  }
}
