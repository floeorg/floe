import { describe, expect, it } from "vitest";
import { build, type Rollup } from "vite";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import floe from "./index.ts";

/**
 * The floe binary under test.
 *
 * `FLOE_BIN` wins when it is set, so a person can point the suite at any
 * build. Nothing in CI sets it: the workflow copies the downloaded binary to
 * `target/debug/floe`, which is the second candidate and the one a developer
 * box already has. `floe` on PATH is the last resort.
 */
function resolveCompiler(): string {
  const fromEnv = process.env.FLOE_BIN;
  if (fromEnv) return fromEnv;

  const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
  const debugBinary = join(repoRoot, "target", "debug", "floe");
  if (existsSync(debugBinary)) return debugBinary;

  return "floe";
}

interface FloeProject {
  readonly root: string;
  readonly write: (relativePath: string, contents: string) => void;
}

function createProject(name: string): FloeProject {
  const root = mkdtempSync(join(tmpdir(), `floe-vite-${name}-`));
  const write = (relativePath: string, contents: string) => {
    const target = join(root, relativePath);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, contents);
  };

  write("package.json", `{"name":"${name}","private":true,"type":"module"}\n`);

  return { root, write };
}

/**
 * Bundle `entry` with the Floe plugin and return the entry chunk's code.
 * React stays external so the fixture needs no npm dependencies.
 */
async function bundle(project: FloeProject, entry: string): Promise<string> {
  const result = await build({
    root: project.root,
    configFile: false,
    logLevel: "silent",
    plugins: [floe({ compiler: resolveCompiler() })],
    build: {
      write: false,
      minify: false,
      lib: { entry, formats: ["es"], fileName: "bundle" },
      rollupOptions: { external: [/^react/] },
    },
  });

  const outputs = (Array.isArray(result) ? result : [result]) as Rollup.RollupOutput[];
  const chunk = outputs[0].output.find((item) => item.type === "chunk");
  if (chunk === undefined || chunk.type !== "chunk") {
    throw new Error("the build produced no chunk");
  }

  return chunk.code;
}

describe("@floeorg/vite-plugin", () => {
  it("bundles a .fl component that a hand-written .tsx imports", async () => {
    const project = createProject("tsx-importer");
    project.write(
      "src/greeting.fl",
      `export let Greeting(name: string) -> JSX.Element = {\n` +
        `    <div className="greeting">{\`Hello, \${name}!\`}</div>\n` +
        `}\n`,
    );
    project.write(
      "src/entry.tsx",
      `import { Greeting } from "./greeting";\n\n` +
        `export function App() {\n` +
        `  return <Greeting name="floe" />;\n` +
        `}\n`,
    );

    try {
      const code = await bundle(project, "src/entry.tsx");

      expect(code).toContain(`className: "greeting"`);
      expect(code).toContain("Hello, ");
      expect(code).toContain("App");
    } finally {
      rmSync(project.root, { recursive: true, force: true });
    }
  });

  it("tells the bundler the transformed module is JavaScript", async () => {
    const project = createProject("module-type");
    project.write(
      "src/shout.fl",
      `export let shout(text: string) -> string = {\n` + `    \`\${text}!\`\n` + `}\n`,
    );

    try {
      // Vite types both hooks as `ObjectHook`, so name the plain function
      // shape this plugin uses before calling either one.
      const plugin = floe({ compiler: resolveCompiler() }) as {
        configResolved: (config: { root: string }) => void;
        transform: (
          this: { error(message: string): never },
          code: string,
          id: string,
        ) => Promise<{ code: string; moduleType?: string } | null>;
      };
      const context = {
        error(message: string): never {
          throw new Error(message);
        },
      };

      plugin.configResolved({ root: project.root });
      const result = await plugin.transform.call(
        context,
        "",
        join(project.root, "src", "shout.fl"),
      );

      // Without this, rolldown reads the module type from the `.fl`
      // extension and has to fall back. See the note in index.ts.
      expect(result?.moduleType).toBe("js");
      expect(result?.code).toContain("function shout(");
    } finally {
      rmSync(project.root, { recursive: true, force: true });
    }
  });

  it("bundles a .fl module that another .fl imports", async () => {
    const project = createProject("fl-importer");
    project.write(
      "src/shout.fl",
      `export let shout(text: string) -> string = {\n` + `    \`\${text}!\`\n` + `}\n`,
    );
    project.write(
      "src/greeting.fl",
      `import { shout } from "./shout"\n\n` +
        `export let Greeting(name: string) -> JSX.Element = {\n` +
        `    <div className="greeting">{shout(\`Hello, \${name}\`)}</div>\n` +
        `}\n`,
    );
    project.write(
      "src/entry.tsx",
      `import { Greeting } from "./greeting";\n\n` +
        `export function App() {\n` +
        `  return <Greeting name="floe" />;\n` +
        `}\n`,
    );

    try {
      const code = await bundle(project, "src/entry.tsx");

      expect(code).toContain("function shout(");
      expect(code).toContain("shout(`Hello, ");
      expect(code).toContain(`className: "greeting"`);
    } finally {
      rmSync(project.root, { recursive: true, force: true });
    }
  });
});
