import { describe, it, before, after } from "node:test";
import assert from "node:assert/strict";
import { build } from "esbuild";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

import floe from "./index.ts";

describe("@floeorg/esbuild-plugin", () => {
  let projectDir: string;

  before(() => {
    projectDir = mkdtempSync(join(tmpdir(), "floe-esbuild-test-"));
    mkdirSync(join(projectDir, "src", "nested"), { recursive: true });
    writeFileSync(join(projectDir, "package.json"), `{"name":"test","type":"module"}`);
    writeFileSync(
      join(projectDir, "src", "app.ts"),
      `import { hello } from "./nested/helper";\nexport const out = hello("world");\n`,
    );
    writeFileSync(
      join(projectDir, "src", "nested", "helper.fl"),
      `export let hello(name: string) -> string = {\n    \`Hello, \${name}!\`\n}\n`,
    );
    execFileSync("floe", ["build", "src/"], { cwd: projectDir, stdio: "pipe" });
  });

  after(() => {
    rmSync(projectDir, { recursive: true, force: true });
  });

  it("resolves extensionless imports to .fl and bundles the compiled TS", async () => {
    const outfile = join(projectDir, "out.js");
    const result = await build({
      entryPoints: [join(projectDir, "src", "app.ts")],
      bundle: true,
      format: "esm",
      outfile,
      plugins: [floe()],
      absWorkingDir: projectDir,
      logLevel: "silent",
    });
    assert.equal(result.errors.length, 0);

    const bundle = readFileSync(outfile, "utf8");
    assert.match(bundle, /Hello, \$\{name\}/);
    assert.match(bundle, /hello\("world"\)/);
  });

  it("respects user-written .ts siblings over .fl", async () => {
    mkdirSync(join(projectDir, "src", "ts-wins"), { recursive: true });
    writeFileSync(
      join(projectDir, "src", "ts-wins", "shared.ts"),
      `export const kind = "ts";\n`,
    );
    writeFileSync(
      join(projectDir, "src", "ts-wins", "shared.fl"),
      `export let _kind: string = "fl"\n`,
    );
    writeFileSync(
      join(projectDir, "src", "entry.ts"),
      `import { kind } from "./ts-wins/shared";\nexport const out = kind;\n`,
    );
    execFileSync("floe", ["build", "src/"], { cwd: projectDir, stdio: "pipe" });

    const outfile = join(projectDir, "out2.js");
    await build({
      entryPoints: [join(projectDir, "src", "entry.ts")],
      bundle: true,
      format: "esm",
      outfile,
      plugins: [floe()],
      absWorkingDir: projectDir,
      logLevel: "silent",
    });

    const bundle = readFileSync(outfile, "utf8");
    assert.match(bundle, /kind = "ts"/);
    assert.doesNotMatch(bundle, /_kind = "fl"/);
  });

});
