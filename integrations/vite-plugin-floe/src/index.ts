import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
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
    name: "vite-plugin-floe",
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

        const compiled = compileFloe(compiler, code, id);
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

/**
 * Read compiled .ts/.tsx output from .floe/ if it exists and is fresh
 * (newer than the source .fl file). Returns null if missing or stale.
 */
function readCompiledOutput(
  flFile: string,
  projectRoot: string,
): string | null {
  const rel = relative(projectRoot, flFile);
  const floeDir = join(projectRoot, ".floe");

  let sourceMtime: number;
  try {
    sourceMtime = statSync(flFile).mtimeMs;
  } catch {
    return null;
  }

  for (const ext of ["tsx", "ts"]) {
    const outPath = join(floeDir, rel).replace(/\.fl$/, `.${ext}`);
    try {
      if (statSync(outPath).mtimeMs >= sourceMtime) {
        return readFileSync(outPath, "utf-8");
      }
    } catch {
      // File doesn't exist, try next extension
    }
  }

  return null;
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
