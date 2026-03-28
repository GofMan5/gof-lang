import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const {
  buildCheckInvocation,
  filterDiagnosticsForDocument,
  normalizeDiagnosticsConfig,
  parseCheckReport
} = require("../lib/diagnostics.cjs");

test("configured toolchain path wins over all fallbacks", () => {
  const workspaceRoot = path.join(os.tmpdir(), "gof-vscode-configured");
  const documentPath = path.join(workspaceRoot, "app", "main.gof");
  const configuredToolchain = path.join(workspaceRoot, "bin", "gof");
  const invocation = buildCheckInvocation({
    documentPath,
    workspaceFolder: workspaceRoot,
    config: {
      toolchainPath: configuredToolchain,
      useCargoFallback: true
    }
  });

  assert.equal(invocation.command, configuredToolchain);
  assert.deepEqual(invocation.args, ["check", documentPath, "--json", "--stdin"]);
  assert.equal(invocation.cwd, workspaceRoot);
  assert.equal(invocation.mode, "configured-path");
});

test("cargo fallback is selected inside the gof repository", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "gof-vscode-"));
  const nestedWorkspace = path.join(root, "tools", "vscode-gof");
  await mkdir(path.join(root, "tools", "gof-cli"), { recursive: true });
  await mkdir(nestedWorkspace, { recursive: true });
  await writeFile(path.join(root, "tools", "gof-cli", "Cargo.toml"), "[package]\nname = \"gof-cli\"\n");

  const invocation = buildCheckInvocation({
    documentPath: path.join(nestedWorkspace, "sample.gof"),
    workspaceFolder: nestedWorkspace,
    config: {
      toolchainPath: "",
      useCargoFallback: true
    }
  });

  assert.equal(invocation.command, "cargo");
  assert.equal(invocation.cwd, root);
  assert.equal(invocation.mode, "cargo-fallback");
  assert.deepEqual(invocation.args.slice(0, 7), [
    "run",
    "-q",
    "-p",
    "gof-cli",
    "--bin",
    "gof",
    "--"
  ]);
  assert.deepEqual(invocation.args.slice(7), [
    "check",
    path.join(nestedWorkspace, "sample.gof"),
    "--json",
    "--stdin"
  ]);
});

test("plain PATH invocation is used outside the repository", () => {
  const workspaceRoot = path.join(os.tmpdir(), "gof-vscode-path");
  const documentPath = path.join(workspaceRoot, "sample.gof");
  const invocation = buildCheckInvocation({
    documentPath,
    workspaceFolder: workspaceRoot,
    config: {
      toolchainPath: "",
      useCargoFallback: true
    }
  });

  assert.equal(invocation.command, "gof");
  assert.equal(invocation.cwd, workspaceRoot);
  assert.equal(invocation.mode, "path");
});

test("parseCheckReport validates and normalizes diagnostics", () => {
  const report = parseCheckReport(
    JSON.stringify({
      ok: false,
      input: "/workspace/main.gof",
      diagnostics: [
        {
          code: "GOF3013",
          severity: "error",
          message: "type mismatch",
          note: "expected int",
          fixIt: "change the value",
          sourcePath: "/workspace/main.gof",
          line: 3,
          column: 5,
          endColumn: 9
        }
      ]
    })
  );

  assert.equal(report.ok, false);
  assert.equal(report.input, "/workspace/main.gof");
  assert.equal(report.diagnostics[0].code, "GOF3013");
  assert.equal(report.diagnostics[0].line, 3);
  assert.equal(report.diagnostics[0].endColumn, 9);
});

test("filterDiagnosticsForDocument keeps only diagnostics for the active file", () => {
  const report = parseCheckReport(
    JSON.stringify({
      ok: false,
      input: "/workspace/main.gof",
      diagnostics: [
        {
          code: "GOF3013",
          severity: "error",
          message: "main mismatch",
          sourcePath: "/workspace/main.gof",
          line: 2,
          column: 1,
          endColumn: 4
        },
        {
          code: "GOF3008",
          severity: "error",
          message: "import mismatch",
          sourcePath: "/workspace/lib/math.gof",
          line: 1,
          column: 1,
          endColumn: 3
        }
      ]
    })
  );

  const diagnostics = filterDiagnosticsForDocument(report, "/workspace/main.gof");
  assert.equal(diagnostics.length, 1);
  assert.equal(diagnostics[0].code, "GOF3013");
});

test("normalizeDiagnosticsConfig clamps and defaults settings", () => {
  assert.deepEqual(normalizeDiagnosticsConfig({ debounceMs: -10 }), {
    enabled: true,
    debounceMs: 0,
    runOnSaveOnly: false,
    toolchainPath: "",
    useCargoFallback: true
  });
});
