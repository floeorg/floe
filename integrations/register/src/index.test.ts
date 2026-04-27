import { describe, it, before, after } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve as resolvePath } from "node:path";
import { execFileSync } from "node:child_process";

const REGISTER_DIST = resolvePath(import.meta.dirname, "..", "dist", "index.js");

describe("@floeorg/register", () => {
  let projectDir: string;

  before(() => {
    projectDir = mkdtempSync(join(tmpdir(), "floe-register-test-"));
    mkdirSync(join(projectDir, "src", "providers"), { recursive: true });
    writeFileSync(
      join(projectDir, "package.json"),
      `{"name":"test","type":"module"}`,
    );

    // Floe module that imports a hand-written .ts sibling. The shim
    // exposes a plain Node API; the .fl module wraps it.
    writeFileSync(
      join(projectDir, "src", "providers", "thing-state.ts"),
      `export function getThing(): string { return "shim-value"; }\n`,
    );
    writeFileSync(
      join(projectDir, "src", "providers", "thing.fl"),
      `import trusted { getThing } from "./thing-state"

export let read() -> string = {
    getThing()
}
`,
    );

    // Entry point: a plain TS file that imports the .fl module.
    writeFileSync(
      join(projectDir, "src", "main.ts"),
      `import { read } from "./providers/thing.fl";
console.log(read());
`,
    );

    execFileSync("floe", ["build", "src/"], {
      cwd: projectDir,
      stdio: "pipe",
    });
  });

  after(() => {
    rmSync(projectDir, { recursive: true, force: true });
  });

  it("falls back to src/ when a compiled .fl.ts imports a hand-written .ts sibling", () => {
    const stdout = execFileSync(
      "node",
      ["--import", `file://${REGISTER_DIST}`, "src/main.ts"],
      { cwd: projectDir, encoding: "utf8" },
    );
    assert.equal(stdout.trim(), "shim-value");
  });
});
