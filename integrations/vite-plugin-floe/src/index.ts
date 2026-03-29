import { execFileSync } from "node:child_process";

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
export default function floe(options: FloeOptions = {}) {
  const compiler = options.compiler ?? "floe";

  return {
    name: "vite-plugin-floe",
    enforce: "pre" as const,

    config(config: { resolve?: { extensions?: string[] } }) {
      const existing = config.resolve?.extensions ?? [".mjs", ".js", ".mts", ".ts", ".jsx", ".tsx", ".json"];
      const extensions = existing.includes(".fl") ? existing : [".fl", ...existing];
      return {
        resolve: { extensions },
        esbuild: {
          include: /\.(tsx?|jsx?|fl)$/,
          loader: "tsx" as const,
        },
      };
    },

    transform(this: { error(msg: string): never }, code: string, id: string) {
      // Strip query params for extension check (Vite adds ?import, ?t=xxx, etc.)
      const cleanId = id.split("?")[0];
      if (!cleanId.endsWith(".fl")) return null;

      try {
        const result = compileFloe(compiler, code, id);
        return {
          code: result.code,
          map: result.map,
          moduleType: "tsx",
        };
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
