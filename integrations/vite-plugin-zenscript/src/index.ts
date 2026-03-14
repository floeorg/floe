import { execFileSync } from "node:child_process";
import type { Plugin } from "vite";

export interface ZenScriptOptions {
  /** Path to the zsc binary. Defaults to "zsc". */
  compiler?: string;
}

/**
 * Vite plugin for ZenScript.
 *
 * Transforms `.zs` files to TypeScript in the build pipeline.
 * Uses the `zsc` compiler binary for compilation.
 *
 * @example
 * ```ts
 * import { defineConfig } from "vite"
 * import zenscript from "vite-plugin-zenscript"
 *
 * export default defineConfig({
 *   plugins: [zenscript()],
 * })
 * ```
 */
export default function zenscript(options: ZenScriptOptions = {}): Plugin {
  const compiler = options.compiler ?? "zsc";

  return {
    name: "vite-plugin-zenscript",
    enforce: "pre",

    transform(code, id) {
      if (!id.endsWith(".zs")) return null;

      try {
        const result = compileZenScript(compiler, code, id);
        return {
          code: result.code,
          map: result.map,
        };
      } catch (error) {
        const message =
          error instanceof Error ? error.message : String(error);
        this.error(`ZenScript compilation failed for ${id}:\n${message}`);
      }
    },

    handleHotUpdate({ file, server }) {
      if (file.endsWith(".zs")) {
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

function compileZenScript(
  compiler: string,
  source: string,
  filename: string,
): CompileResult {
  try {
    const output = execFileSync(compiler, ["build", "--emit-stdout", "-"], {
      input: source,
      encoding: "utf-8",
      timeout: 30_000,
      env: {
        ...process.env,
        ZSC_FILENAME: filename,
      },
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
