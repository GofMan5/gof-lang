"use strict";

const { spawn } = require("node:child_process");
const vscode = require("vscode");

const {
  buildCheckInvocation,
  filterDiagnosticsForDocument,
  isGofDocument,
  normalizeDiagnosticsConfig,
  parseCheckReport
} = require("./lib/diagnostics.cjs");

function activate(context) {
  const diagnostics = vscode.languages.createDiagnosticCollection("gof");
  const output = vscode.window.createOutputChannel("gof Diagnostics");
  const state = {
    diagnostics,
    output,
    timers: new Map(),
    generations: new Map()
  };

  context.subscriptions.push(diagnostics, output);
  context.subscriptions.push(
    vscode.commands.registerCommand("gof.showDiagnosticsOutput", () => {
      output.show(true);
    }),
    vscode.commands.registerCommand("gof.recheckActiveDocument", async () => {
      const document = vscode.window.activeTextEditor?.document;
      if (!isGofDocument(document)) {
        vscode.window.showWarningMessage("Open a saved .gof file to re-run gof diagnostics.");
        return;
      }
      scheduleDocumentDiagnostics(document, state, "manual");
    }),
    vscode.workspace.onDidOpenTextDocument((document) => {
      scheduleDocumentDiagnostics(document, state, "open");
    }),
    vscode.workspace.onDidChangeTextDocument((event) => {
      scheduleDocumentDiagnostics(event.document, state, "change");
    }),
    vscode.workspace.onDidSaveTextDocument((document) => {
      scheduleDocumentDiagnostics(document, state, "save");
    }),
    vscode.workspace.onDidCloseTextDocument((document) => {
      clearDocumentState(document.uri, state);
    }),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (!event.affectsConfiguration("gof.diagnostics") && !event.affectsConfiguration("gof.toolchain")) {
        return;
      }
      appendOutput(state, "Configuration changed; refreshing gof diagnostics.");
      for (const document of vscode.workspace.textDocuments) {
        if (isGofDocument(document)) {
          scheduleDocumentDiagnostics(document, state, "config");
        }
      }
    })
  );

  for (const document of vscode.workspace.textDocuments) {
    if (isGofDocument(document)) {
      scheduleDocumentDiagnostics(document, state, "open");
    }
  }
}

function deactivate() {}

function scheduleDocumentDiagnostics(document, state, reason) {
  if (!isGofDocument(document)) {
    return;
  }

  const config = readDiagnosticsConfig();
  if (!config.enabled) {
    clearDocumentState(document.uri, state);
    return;
  }
  if (config.runOnSaveOnly && reason === "change") {
    return;
  }

  const key = document.uri.toString();
  const currentGeneration = (state.generations.get(key) ?? 0) + 1;
  state.generations.set(key, currentGeneration);

  const existingTimer = state.timers.get(key);
  if (existingTimer) {
    clearTimeout(existingTimer);
  }

  const delay = reason === "manual" || reason === "save" ? 0 : config.debounceMs;
  const timer = setTimeout(() => {
    state.timers.delete(key);
    runDocumentDiagnostics(document, state, currentGeneration).catch((error) => {
      appendOutput(state, `unexpected diagnostics failure for ${document.uri.fsPath}: ${error.stack || error.message}`);
      publishExtensionWarning(
        document,
        state,
        "GOFEXT003",
        `Unexpected gof diagnostics failure: ${error.message}`
      );
    });
  }, delay);

  state.timers.set(key, timer);
}

async function runDocumentDiagnostics(document, state, generation) {
  const key = document.uri.toString();
  const config = readDiagnosticsConfig();
  if (!config.enabled || state.generations.get(key) !== generation) {
    return;
  }

  const workspaceFolder = vscode.workspace.getWorkspaceFolder(document.uri)?.uri.fsPath;
  const invocation = buildCheckInvocation({
    documentPath: document.uri.fsPath,
    workspaceFolder,
    config
  });
  appendOutput(
    state,
    `checking ${document.uri.fsPath} via ${formatInvocation(invocation)}`
  );

  let processResult;
  try {
    processResult = await runCheckProcess(invocation, document.getText());
  } catch (error) {
    if (state.generations.get(key) !== generation) {
      return;
    }
    appendOutput(
      state,
      `toolchain launch failed for ${document.uri.fsPath}: ${error.stack || error.message}`
    );
    publishExtensionWarning(document, state, "GOFEXT001", buildToolchainFailureMessage(config, error));
    return;
  }

  if (state.generations.get(key) !== generation) {
    return;
  }

  let report;
  try {
    report = parseCheckReport(processResult.stdout);
  } catch (error) {
    appendOutput(
      state,
      `invalid diagnostics payload for ${document.uri.fsPath} (exit ${processResult.code}): ${error.message}`
    );
    if (processResult.stderr.trim()) {
      appendOutput(state, processResult.stderr.trim());
    }
    publishExtensionWarning(
      document,
      state,
      "GOFEXT002",
      "The gof toolchain returned invalid JSON for `gof check --json`. Open `gof Diagnostics` for details."
    );
    return;
  }

  if (processResult.stderr.trim()) {
    appendOutput(state, processResult.stderr.trim());
  }
  if (processResult.code > 1) {
    appendOutput(
      state,
      `diagnostics command exited with code ${processResult.code} while still returning a report`
    );
  }

  const ownDiagnostics = filterDiagnosticsForDocument(report, document.uri.fsPath).map((diagnostic) =>
    toVsCodeDiagnostic(diagnostic, document)
  );
  state.diagnostics.set(document.uri, ownDiagnostics);
}

function runCheckProcess(invocation, sourceText) {
  return new Promise((resolve, reject) => {
    const child = spawn(invocation.command, invocation.args, {
      cwd: invocation.cwd,
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true
    });

    let stdout = "";
    let stderr = "";

    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("close", (code) => {
      resolve({
        code: typeof code === "number" ? code : -1,
        stdout,
        stderr
      });
    });

    child.stdin.on("error", () => {});
    child.stdin.end(sourceText, "utf8");
  });
}

function toVsCodeDiagnostic(diagnostic, document) {
  const lineIndex = clampLine(document, diagnostic.line - 1);
  const startColumn = clampColumn(document, lineIndex, diagnostic.column - 1);
  const requestedEnd = Math.max(startColumn + 1, diagnostic.endColumn - 1);
  const endColumn = clampColumn(document, lineIndex, requestedEnd);
  const range = new vscode.Range(lineIndex, startColumn, lineIndex, endColumn);
  const severity =
    diagnostic.severity === "warning"
      ? vscode.DiagnosticSeverity.Warning
      : vscode.DiagnosticSeverity.Error;
  const message = renderDiagnosticMessage(diagnostic);
  const vsDiagnostic = new vscode.Diagnostic(range, message, severity);
  vsDiagnostic.code = diagnostic.code;
  vsDiagnostic.source = "gof check";
  return vsDiagnostic;
}

function renderDiagnosticMessage(diagnostic) {
  const sections = [diagnostic.message];
  if (diagnostic.note) {
    sections.push(`note: ${diagnostic.note}`);
  }
  if (diagnostic.fixIt) {
    sections.push(`fix-it: ${diagnostic.fixIt}`);
  }
  return sections.join("\n");
}

function publishExtensionWarning(document, state, code, message) {
  const range = new vscode.Range(0, 0, 0, 0);
  const diagnostic = new vscode.Diagnostic(range, message, vscode.DiagnosticSeverity.Warning);
  diagnostic.code = code;
  diagnostic.source = "gof extension";
  state.diagnostics.set(document.uri, [diagnostic]);
}

function clearDocumentState(uri, state) {
  const key = uri.toString();
  const timer = state.timers.get(key);
  if (timer) {
    clearTimeout(timer);
    state.timers.delete(key);
  }
  state.generations.delete(key);
  state.diagnostics.delete(uri);
}

function readDiagnosticsConfig() {
  const settings = vscode.workspace.getConfiguration("gof");
  return normalizeDiagnosticsConfig({
    enabled: settings.get("diagnostics.enabled", true),
    debounceMs: settings.get("diagnostics.debounceMs", 250),
    runOnSaveOnly: settings.get("diagnostics.runOnSaveOnly", false),
    toolchainPath: settings.get("toolchain.path", ""),
    useCargoFallback: settings.get("toolchain.useCargoFallback", true)
  });
}

function buildToolchainFailureMessage(config, error) {
  if (config.toolchainPath) {
    return `Failed to start the configured gof toolchain at \`${config.toolchainPath}\`: ${error.message}`;
  }
  return "Failed to start the gof diagnostics toolchain. Set `gof.toolchain.path` or open the gof repository so the extension can use cargo fallback.";
}

function formatInvocation(invocation) {
  return `${invocation.command} ${invocation.args.join(" ")}`;
}

function appendOutput(state, message) {
  state.output.appendLine(`[${new Date().toISOString()}] ${message}`);
}

function clampLine(document, line) {
  if (document.lineCount === 0) {
    return 0;
  }
  return Math.max(0, Math.min(line, document.lineCount - 1));
}

function clampColumn(document, line, column) {
  const textLine = document.lineAt(clampLine(document, line));
  return Math.max(0, Math.min(column, textLine.text.length));
}

module.exports = {
  activate,
  deactivate
};
