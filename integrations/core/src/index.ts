import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

export interface CompileResult {
  code: string;
  map: string | null;
}

export interface CompileOptions {
  /** Path to the floe binary. Defaults to "floe". */
  compiler?: string;
}

/**
 * Read compiled .ts/.tsx output from .floe/ if it exists and is fresh
 * (newer than the source .fl file). Returns null if missing or stale.
 */
export function readCompiledOutput(
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

/**
 * Compile a .fl file by shelling out to the floe CLI.
 * Used as a fallback when .floe/ output is missing or stale.
 */
export function compileFloe(
  compiler: string,
  filename: string,
): CompileResult {
  try {
    const output = execFileSync(compiler, ["build", "--emit-stdout", filename], {
      encoding: "utf-8",
      timeout: 30_000,
      stdio: ["pipe", "pipe", "pipe"],
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

/**
 * Resolve a .fl file to its compiled output. Tries .floe/ first,
 * falls back to compiling via CLI.
 */
export function resolveFloeFile(
  flFile: string,
  projectRoot: string,
  options: CompileOptions = {},
): string {
  const cached = readCompiledOutput(flFile, projectRoot);
  if (cached) return cached;

  const compiler = options.compiler ?? "floe";
  const result = compileFloe(compiler, flFile);
  return result.code;
}
