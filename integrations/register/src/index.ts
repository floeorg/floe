import { registerHooks } from "node:module";
import { statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/**
 * Find the project root by walking up to find node_modules or package.json.
 * Matches the Floe compiler's `find_project_dir` logic.
 */
function findProjectRoot(start: string): string {
  let dir = start;
  let packageJsonDir: string | null = null;
  while (true) {
    try {
      statSync(join(dir, "node_modules"));
      return dir;
    } catch {}
    if (packageJsonDir === null) {
      try {
        statSync(join(dir, "package.json"));
        packageJsonDir = dir;
      } catch {}
    }
    const parent = dirname(dir);
    if (parent === dir) return packageJsonDir ?? start;
    dir = parent;
  }
}

/**
 * Find the compiled .ts/.tsx output in .floe/ for a given .fl file.
 * Returns the absolute path if found, null otherwise.
 */
function findCompiledPath(
  flFile: string,
  projectRoot: string,
): string | null {
  const rel = relative(projectRoot, flFile);
  const floeDir = join(projectRoot, ".floe");

  for (const ext of ["tsx", "ts"]) {
    const outPath = join(floeDir, rel).replace(/\.fl$/, `.${ext}`);
    try {
      statSync(outPath);
      return outPath;
    } catch {}
  }
  return null;
}

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (!specifier.endsWith(".fl")) {
      return nextResolve(specifier, context);
    }

    // Only handle relative imports — bare specifiers go through normal resolution
    if (!specifier.startsWith(".")) {
      return nextResolve(specifier, context);
    }

    const parentPath = context.parentURL
      ? fileURLToPath(context.parentURL)
      : join(process.cwd(), "__entry__");
    const flFile = resolve(dirname(parentPath), specifier);
    const projectRoot = findProjectRoot(dirname(flFile));
    const compiled = findCompiledPath(flFile, projectRoot);

    if (compiled) {
      return {
        url: pathToFileURL(compiled).href,
        shortCircuit: true,
      };
    }

    throw new Error(
      `Floe: compiled output not found for "${specifier}". ` +
        `Run \`floe build\` or \`floe watch\` first.`,
    );
  },
});
