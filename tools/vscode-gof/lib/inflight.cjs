"use strict";

const { spawn } = require("node:child_process");

class DiagnosticsRunCancelledError extends Error {
  constructor(message = "gof diagnostics run cancelled") {
    super(message);
    this.name = "DiagnosticsRunCancelledError";
  }
}

function startCheckProcess(invocation, sourceText, spawnImpl = spawn) {
  const child = spawnImpl(invocation.command, invocation.args, {
    cwd: invocation.cwd,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true
  });

  let stdout = "";
  let stderr = "";
  let cancelled = false;
  let cancelReason = "gof diagnostics run cancelled";

  const promise = new Promise((resolve, reject) => {
    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", (error) => {
      if (cancelled) {
        reject(new DiagnosticsRunCancelledError(cancelReason));
        return;
      }
      reject(error);
    });
    child.on("close", (code) => {
      if (cancelled) {
        reject(new DiagnosticsRunCancelledError(cancelReason));
        return;
      }
      resolve({
        code: typeof code === "number" ? code : -1,
        stdout,
        stderr
      });
    });

    child.stdin.on("error", () => {});
    child.stdin.end(sourceText, "utf8");
  });

  return {
    promise,
    cancel(reason = "gof diagnostics run cancelled") {
      cancelReason = reason;
      cancelled = true;
      if (typeof child.kill === "function" && !child.killed) {
        try {
          child.kill();
        } catch {}
      }
    }
  };
}

function cancelInflight(state, key, reason = "gof diagnostics run cancelled") {
  const inflight = state.inflight.get(key);
  if (!inflight) {
    return;
  }
  state.inflight.delete(key);
  inflight.cancel(reason);
}

function replaceInflight(state, key, inflight, reason = "gof diagnostics run superseded") {
  cancelInflight(state, key, reason);
  state.inflight.set(key, inflight);
}

function settleInflight(state, key, inflight) {
  if (state.inflight.get(key) === inflight) {
    state.inflight.delete(key);
  }
}

function isExpectedCancellation(error) {
  return error instanceof DiagnosticsRunCancelledError;
}

module.exports = {
  DiagnosticsRunCancelledError,
  cancelInflight,
  isExpectedCancellation,
  replaceInflight,
  settleInflight,
  startCheckProcess
};
