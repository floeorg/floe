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

/**
 * If `path` lives under `projectRoot/.floe/`, return the corresponding
 * path under `projectRoot/` (the "source mirror"). Returns null when
 * `path` is outside `.floe/`.
 *
 * Used to recover when a compiled `.fl.ts` in `.floe/` imports a
 * hand-written `.ts` sibling: that sibling lives in `src/`, never
 * gets copied into `.floe/`, and Node's default resolver can't find
 * it. Reflecting the parent path back into `src/` lets Node resolve
 * the sibling from its real location.
 */
function srcMirrorOf(path: string, projectRoot: string): string | null {
  const floeDir = join(projectRoot, ".floe");
  const rel = relative(floeDir, path);
  if (rel.startsWith("..") || rel.startsWith("/")) return null;
  return join(projectRoot, rel);
}

/**
 * Probe the filesystem for `target`, trying TS/JS extensions and
 * `index.*` variants. Node ESM strict mode requires explicit
 * extensions on relative imports, but Floe's codegen passes through
 * extensionless source paths unchanged — so once we've reflected the
 * parent back into `src/` we have to do the extension lookup the
 * default resolver would have done in a non-strict world.
 */
function probeExtensions(target: string): string | null {
  const candidates = [
    target,
    `${target}.ts`,
    `${target}.tsx`,
    `${target}.mts`,
    `${target}.cts`,
    `${target}.js`,
    `${target}.mjs`,
    `${target}.cjs`,
    join(target, "index.ts"),
    join(target, "index.tsx"),
    join(target, "index.mts"),
    join(target, "index.cts"),
    join(target, "index.js"),
    join(target, "index.mjs"),
    join(target, "index.cjs"),
  ];
  for (const candidate of candidates) {
    try {
      if (statSync(candidate).isFile()) return candidate;
    } catch {}
  }
  return null;
}

registerHooks({
  resolve(specifier, context, nextResolve) {
    // Floe-source import (`./foo.fl`): rewrite to the compiled `.floe/`
    // path. Bare `.fl` specifiers fall through to default resolution.
    if (specifier.endsWith(".fl") && specifier.startsWith(".")) {
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
    }

    // Anything else: try default resolution. If it fails AND the parent
    // is a compiled file under `.floe/`, retry by probing the same
    // relative path from the corresponding `src/` location — that's
    // where hand-written sibling `.ts` files live (they never get
    // copied into `.floe/`).
    try {
      return nextResolve(specifier, context);
    } catch (err) {
      if (
        !specifier.startsWith(".") ||
        !context.parentURL ||
        (err as { code?: string })?.code !== "ERR_MODULE_NOT_FOUND"
      ) {
        throw err;
      }
      const parentPath = fileURLToPath(context.parentURL);
      const projectRoot = findProjectRoot(dirname(parentPath));
      const srcParent = srcMirrorOf(parentPath, projectRoot);
      if (!srcParent) throw err;
      const target = resolve(dirname(srcParent), specifier);
      const probed = probeExtensions(target);
      if (!probed) throw err;
      return {
        url: pathToFileURL(probed).href,
        shortCircuit: true,
      };
    }
  },
});
