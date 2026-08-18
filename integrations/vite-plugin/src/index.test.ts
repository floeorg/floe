import { describe, expect, it } from "vitest";
import { build, type Rollup } from "vite";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import floe from "./index.ts";

/**
 * The floe binary under test. CI downloads the release build and exports
 * `FLOE_BIN`; a developer box uses the debug build this repository produces,
 * and falls back to whatever `floe` is on PATH.
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
