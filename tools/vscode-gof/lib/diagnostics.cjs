"use strict";

const fs = require("node:fs");
const path = require("node:path");

function isGofDocument(document) {
  return (
    !!document &&
    document.languageId === "gof" &&
    !!document.uri &&
    document.uri.scheme === "file"
  );
}

function normalizeDiagnosticsConfig(config) {
  return {
    enabled: config.enabled !== false,
    debounceMs: Math.max(0, Number(config.debounceMs ?? 250) || 0),
    runOnSaveOnly: config.runOnSaveOnly === true,
    toolchainPath: typeof config.toolchainPath === "string" ? config.toolchainPath.trim() : "",
    useCargoFallback: config.useCargoFallback !== false
  };
}

function buildCheckInvocation({ documentPath, workspaceFolder, config }) {
  const normalizedConfig = normalizeDiagnosticsConfig(config);
  const args = ["check", documentPath, "--json", "--stdin"];
  const defaultCwd = workspaceFolder || path.dirname(documentPath);

  if (normalizedConfig.toolchainPath) {
    return {
      command: normalizedConfig.toolchainPath,
      args,
      cwd: defaultCwd,
      mode: "configured-path"
    };
  }

  const cargoRoot = normalizedConfig.useCargoFallback
    ? findCargoFallbackRoot(workspaceFolder || documentPath)
    : null;
  if (cargoRoot) {
    return {
      command: "cargo",
      args: ["run", "-q", "-p", "gof-cli", "--bin", "gof", "--", ...args],
      cwd: cargoRoot,
      mode: "cargo-fallback"
    };
  }

  return {
    command: "gof",
    args,
    cwd: defaultCwd,
    mode: "path"
  };
}

function findCargoFallbackRoot(seedPath) {
  if (!seedPath) {
    return null;
  }

  let current = path.resolve(seedPath);
  try {
    if (fs.statSync(current).isFile()) {
      current = path.dirname(current);
    }
  } catch {
    current = path.dirname(current);
  }

  while (true) {
    const cliManifest = path.join(current, "tools", "gof-cli", "Cargo.toml");
    if (fs.existsSync(cliManifest)) {
      return current;
    }

    const parent = path.dirname(current);
    if (parent === current) {
      return null;
    }
    current = parent;
  }
}

function parseCheckReport(stdout) {
  let parsed;
  try {
    parsed = JSON.parse(stdout);
  } catch (error) {
    throw new Error(`failed to parse \`gof check --json\` output: ${error.message}`);
  }

  if (!parsed || typeof parsed !== "object") {
    throw new Error("`gof check --json` did not return a JSON object");
  }
  if (typeof parsed.ok !== "boolean") {
    throw new Error("`gof check --json` report is missing boolean `ok`");
  }
  if (!Array.isArray(parsed.diagnostics)) {
    throw new Error("`gof check --json` report is missing `diagnostics` array");
  }

  return {
    ok: parsed.ok,
    input: typeof parsed.input === "string" ? parsed.input : "",
    diagnostics: parsed.diagnostics.map(normalizeReportDiagnostic)
  };
}

function normalizeReportDiagnostic(diagnostic) {
  return {
    code: typeof diagnostic?.code === "string" ? diagnostic.code : "GOFEXT000",
    severity: diagnostic?.severity === "warning" ? "warning" : "error",
    message: typeof diagnostic?.message === "string" ? diagnostic.message : "unknown gof diagnostic",
    note: typeof diagnostic?.note === "string" ? diagnostic.note : "",
    fixIt: typeof diagnostic?.fixIt === "string" ? diagnostic.fixIt : "",
    sourcePath: typeof diagnostic?.sourcePath === "string" ? diagnostic.sourcePath : "",
    line: normalizePositiveInteger(diagnostic?.line, 1),
    column: normalizePositiveInteger(diagnostic?.column, 1),
    endColumn: normalizePositiveInteger(diagnostic?.endColumn, normalizePositiveInteger(diagnostic?.column, 1))
  };
}

function filterDiagnosticsForDocument(report, documentPath) {
  const normalizedDocumentPath = normalizeSourcePath(documentPath);
  return report.diagnostics.filter((diagnostic) => {
    if (!diagnostic.sourcePath) {
      return true;
    }
    return normalizeSourcePath(diagnostic.sourcePath) === normalizedDocumentPath;
  });
}

function normalizeSourcePath(sourcePath) {
  if (!sourcePath) {
    return "";
  }
  return path.resolve(sourcePath).replace(/\\/g, "/").toLowerCase();
}

function normalizePositiveInteger(value, fallback) {
  const numeric = Number(value);
  if (Number.isInteger(numeric) && numeric > 0) {
    return numeric;
  }
  return fallback;
}

module.exports = {
  buildCheckInvocation,
  filterDiagnosticsForDocument,
  findCargoFallbackRoot,
  isGofDocument,
  normalizeDiagnosticsConfig,
  normalizeSourcePath,
  parseCheckReport
};
