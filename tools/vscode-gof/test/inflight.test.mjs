import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import test from "node:test";
import { PassThrough } from "node:stream";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const {
  DiagnosticsRunCancelledError,
  cancelInflight,
  isExpectedCancellation,
  replaceInflight,
  settleInflight,
  startCheckProcess
} = require("../lib/inflight.cjs");

function createFakeChild() {
  const child = new EventEmitter();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  child.stdin = new PassThrough();
  child.killed = false;
  child.kill = () => {
    child.killed = true;
    return true;
  };
  return child;
}

test("replaceInflight cancels the previous run before storing the new one", () => {
  const state = { inflight: new Map() };
  const calls = [];
  const first = {
    cancel(reason) {
      calls.push(reason);
    }
  };
  const second = { cancel() {} };

  state.inflight.set("doc", first);
  replaceInflight(state, "doc", second, "superseded");

  assert.deepEqual(calls, ["superseded"]);
  assert.equal(state.inflight.get("doc"), second);
});

test("cancelInflight clears the registry entry and cancels the run", () => {
  const state = { inflight: new Map() };
  const calls = [];
  const inflight = {
    cancel(reason) {
      calls.push(reason);
    }
  };

  state.inflight.set("doc", inflight);
  cancelInflight(state, "doc", "closing");

  assert.deepEqual(calls, ["closing"]);
  assert.equal(state.inflight.has("doc"), false);
});

test("settleInflight only clears the active run", () => {
  const state = { inflight: new Map() };
  const active = { cancel() {} };
  const stale = { cancel() {} };

  state.inflight.set("doc", active);
  settleInflight(state, "doc", stale);
  assert.equal(state.inflight.get("doc"), active);

  settleInflight(state, "doc", active);
  assert.equal(state.inflight.has("doc"), false);
});

test("startCheckProcess resolves stdout, stderr, and exit code", async () => {
  const child = createFakeChild();
  const run = startCheckProcess(
    {
      command: "gof",
      args: ["check"],
      cwd: process.cwd()
    },
    "fn main() -> int:\n    return 0\n",
    () => child
  );

  child.stdout.write("ok");
  child.stderr.write("warn");
  child.emit("close", 1);

  const result = await run.promise;
  assert.deepEqual(result, {
    code: 1,
    stdout: "ok",
    stderr: "warn"
  });
});

test("startCheckProcess rejects with expected cancellation when the run is killed", async () => {
  const child = createFakeChild();
  const run = startCheckProcess(
    {
      command: "gof",
      args: ["check"],
      cwd: process.cwd()
    },
    "fn main() -> int:\n    return 0\n",
    () => child
  );

  run.cancel("superseded");
  child.emit("close", null);

  await assert.rejects(run.promise, (error) => {
    assert.equal(error instanceof DiagnosticsRunCancelledError, true);
    assert.equal(isExpectedCancellation(error), true);
    assert.equal(error.message, "superseded");
    return true;
  });
  assert.equal(child.killed, true);
});
