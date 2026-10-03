# Phase 3 Slice 0 — Concurrency-State Audit (RFC input)

Branch `agent/language-completion`, HEAD `3824ce3`. Read-only audit; no code changed.
Phase 3 goals (VISION.md:82): structured concurrency, full panic/error propagation,
scheduler fairness, stress gates, measured memory/latency model.

All anchors are `compiler/gof-compiler/src/interpreter.rs` unless prefixed.

## 1. Current surface inventory (code-verified)

### Task creation — `go expr`
- Grammar: `go some_fn(...)`, `go generic_fn(...)`, `go closure_value(...)`; anything else → GOF3008 (eval arm `Expr::Go` 6256–6337; typed rule typed_hir.rs:5661–5720; spec 468).
- `spawn_task` (6809–6868): **one OS thread per task** — plain `std::thread::spawn` (6851, 6905). No scheduler, no pool, no M:N mapping. Task thread gets the **default ~2 MB stack** (NOT the 256 MB interpreter budget from pipeline.rs:152).
- Boundary classification (6834): spawned fn's declared return type is `Result[..., RuntimeError]` → `TaskBoundary::RuntimeResult`, else `TaskBoundary::Direct`.
- Task body runs under `std::panic::catch_unwind` (6852): outcomes are `Value` / `Diagnostics` (gof runtime error) / `Panic` (Rust-level panic; only `function_name` + `source_path` + `span` are kept, 6859–6863).
- Arg capture vs RFC 0043 (§143–155): args are evaluated at the spawn site (6307–6312); an identifier read is `binding.value.clone()` (4969–4971). Handles (channel/task/token) cross as-is via `Arc` ✓. Value payloads get an **unconditional eager deep clone** — `Value::List(Vec<Value>)` (122–148) has no CoW and there is no refcount-1 transfer fast path, so RFC 0043's "transfer if unique / copy if shared" is approximated by "always clone at full cost". Owned taxon (`Box[T]`) not yet implemented in the evaluator.
- Const table crosses as an owned clone; functions/methods/structs/enums are Arc-shared; `OutputBuffer` is shared (task prints land in the shared buffer).

### Joining — `await` / `await_result`
- `await task` (6338–6353; `TaskHandle::await_value` 1856–1875): blocks via condvar `WaitSignal` (1809–1854), not polling.
  - `Direct` task: value → value; task's gof failure → **the raw diagnostics re-raise at the join site** (probe a1: GOF3068 with span inside the task body, CLI exit 1); task Rust-panic → `GOF3010 "task `name` panicked"` (2494–2505).
  - `RuntimeResult` task: failure → `Result.Err(RuntimeError.TaskFailed(msg))` (all diagnostic codes+messages concatenated, 2481–2492); panic → `Result.Err(RuntimeError.TaskPanicked(task))` (2348–2354).
- `await_result(task[, token])` (11173–11238; `await_result_value` 1877–1888): always recoverable `Result`; join-token cancelled before completion → `Result.Err(RuntimeError.Cancelled)`; blocked joins register the token's WaitSignal and wake directly (1828–1830).
- Non-task operand → GOF3009.

### Channels (ChannelHandle 1891–2199)
- `channel()` → **unbounded** queue (capacity `None`; `send` never blocks, 2152–2156) — no backpressure by default (spec 476).
- `channel(0)` → rendezvous: `send` blocks until a receiver takes the value (`rendezvous_slot`, 2014–2024).
- `channel(n)` → bounded: `send` blocks while full (1966–1972, 2026–2036). Negative capacity → GOF3097.
- `send(ch, v[, token])` (10516–10598) → `Result[unit, RuntimeError]`; **send-after-close → `Err(ChannelClosed)`** (checked first, 1999–2005) — no panic, no hang. Blocked send woken by close → retracts the rendezvous slot and returns `Err` (2000–2003).
- `recv(ch[, token])` (10600–10668) → `Result[value, RuntimeError]`; **queued values are drained after close, then `Err(ChannelClosed)`** (2068–2087).
- `close(ch)` (10463–10514): sets `closed`, wakes all waiters; effectively idempotent; returns `unit` (not `Result`; non-channel → GOF3081).
- All blocking waits are condvar-based (`WaitSignal` generation counter, 1024–1065) — no sleep-polling in channel ops.

### `select` (Stmt::Select 4710–4853; PreparedSelectOperation 10670–10938)
- Arms: `recv`/`send` forms, optional binding, optional token; at most one `default` (GOF3095). **No timeout arm** — timeouts arrive only as token-armed arms via `timeout_token`.
- Arm channel/value expressions are prepared **once at select-entry** (matches RFC 0043's "only the winning send arm moves its payload").
- Loop: poll all arms once in rotating order → none ready + `default` → default body immediately → else block on one shared `WaitSignal` registered on all arm channels + tokens, re-poll on wake.
- Round-robin: thread-local `NEXT_SELECT_ARM_START` (36–51) advances one step per blocking pass — deterministic rotation among already-ready arms only; spec 495 admits "stronger scheduler-level fairness is still not guaranteed".
- Rendezvous channels register select-receivers as waiting receivers (`select_waiting_receivers`, 1918–1956) so select-recv unblocks plain senders.

### Cancellation (CancelTokenValue 1103–1173)
- `cancel_token()`, `cancel(token)` (idempotent; wakes registered waiters 1124–1135), `is_cancelled(token)`, `timeout_token(ms)`, `cancel_after(token, ms)`.
- **Cooperative only** — a running task's computation is never interrupted; cancellation is observed only at channel / send / recv / await_result / stream wait boundaries.
- `timeout_token`/`cancel_after` each spawn a dedicated **timer OS thread that sleeps then cancels** (1161–1165).
- Net I/O with cancel/deadline control **polls in 25 ms sleep slices** (accept path 12191–12211; `control_poll_slice` 11580–11595) — bounded polling, up to 25 ms cancellation latency on sockets.
- The 3 flaky tests `*_cancellation_wakes_without_polling_floor` (16493, 17200, 17229) verify **at the Rust primitive level** (TaskHandle/ChannelHandle directly, no gof source) that cancelling a token wakes a blocked `await_result` / `send` / `recv` through direct notification rather than a fixed polling floor. Each runs 10/6/6 iterations of (spawn waiter thread → sleep 2 ms → cancel → join) and asserts total elapsed < 50 ms. They are flaky because they are **wall-clock asserts in the default debug suite** — exactly the pattern BUDGETS.md:96 bans for gates ("wall-clock assertions never run in the default test suite"); the fixed 2 ms sleeps alone consume 12–20 ms of the 50 ms budget, and thread spawn/join jitter on loaded Windows CI can exceed the rest even when the wake path is correct. (This is the Phase-2 freeze-audit caveat recorded in the worklog.)

### Panic propagation today
- No user-level `panic` construct exists in v1 (spec 22, 499 — "panic reserved for invariant failures").
- "Task panic" = internal evaluator Rust panic caught by `catch_unwind` → `TaskPanic` (function_name, source_path, span — **no message/cause**). Reachable from user code only via exotic paths; exercised in unit tests by injecting `store_panic` directly (16397–16430, 16456–16470). No CLI fixture triggers it; no runtime-fail fixture for GOF3010.
- **Stack overflow is not catchable**: a task (or the interpreter itself) that exceeds its stack aborts the whole process (0xC00000FD, "thread '<unknown>' has overflowed its stack", no gof diagnostic).
- Unjoined failing/panicking task: **completely silent** (probe a3b).

## 2. Gap map against Phase 3 goals

### Structured concurrency — none exists
- Language surface has exactly `Expr::Go` / `Expr::Await` (ast.rs:380–384); zero occurrences of task-group/scope/join_all/nursery anywhere in the compiler.
- Tasks are **detached by default**: RFC 0043:163–165 fixes "dropping a task handle detaches the task". No join-at-scope-exit, no cancel-children-on-parent-exit, no error-cancels-siblings, no scope object at any layer.
- **Program exit never waits for outstanding tasks**: `run_with_output_with_args_and_optional_stdin` (3278–3365) evaluates `main` and returns immediately; the CLI's big-stack interpreter thread ends and the process tears down, killing running task threads mid-flight and losing their buffered output (probe b).
- 10 k detached spawns work (probe d) but nothing joins or bounds them; a failing task among 10 k detached ones is unobservable.

### Panic/error propagation — concrete holes
1. **Unjoined task failure is swallowed with no exit-time report** (probe a3b: task divides by zero, main sleeps 300 ms so the failure definitely happens, then exits 0 silently).
2. **Parent/process exit kills children silently** (probe b: child's first print never even appeared; exit 0).
3. **Stack asymmetry**: interpreter thread 256 MB (pipeline.rs:152) vs task threads default ~2 MB → a task crashes the process at ~10–19 gof recursion frames (debug) while `main` supports 2000+ (probes). The shipped GOF3178 recursion-budget contract does not cover spawned tasks. Budget overflow is a raw abort even on the main path (depth 3000 crashes; depth 2000 is fine).
4. `TaskPanic`/`RuntimeError.TaskPanicked` carry **only the task name** — no message, code, or cause chain; GOF3010's note is generic (2494–2505).
5. Plain `await` on a `Direct` task re-raises the raw diagnostic with no task-boundary marker — same shape as an inline failure; propagation shape differs between `await` (re-raise) and `await_result` (wrap) for the same event.
6. Boundary classification keys off the **declared** return type, so `task[Result[int, RuntimeError]]` vs plain `task[T]` change the propagation contract silently at the join site.

### Scheduler fairness
- There is no gof scheduler at all: thread-per-task delegates fairness to the OS. 10 k concurrent threads completed (probe d, 23.8 s debug), so no starvation found — but no guarantee is specified or tested either.
- The only in-language fairness mechanism is `select`'s per-pass round-robin rotation (36–51); spec 495 explicitly disclaims stronger fairness.
- Channel/task waits are condvar-based (no busy loops) — the real polling lives in net I/O (25 ms slices).
- Roadmap: M5 "scheduler stress and concurrency benchmarks" `[ ]`, M8 "stress harness for runtime and concurrency" `[ ]`, M10 "stronger scheduling guarantees and fairness rules" `[~]`; RFC 0051's Phase-3 revisit gate (rfcs/0051:340–352) demands measured latency/fairness evidence before any async reopening.
- The 3 wake-floor tests are the only timing-sensitive checks — and they are the repo's known flakies (wall-clock asserts in the default suite).

### Measurement
- `benchmarks/gof-bench/benches/concurrency_runtime.rs` contains **exactly one criterion bench**: "runtime blocking rendezvous select" — one end-to-end `run_with_output` of a 2-task rendezvous select, inclusive of module setup. Nothing else.
- Missing for the Phase-3 "measured memory/latency model": per-task spawn/join latency; per-task memory cost (thread + default stack + TaskHandle + eagerly cloned args); channel op throughput; **wake/cancellation latency distribution** (what the flaky tests probe informally); unbounded vs bounded vs rendezvous comparison; 10 k-task stress wall-clock gate; RFC 0043:290's explicitly listed "boundary deep-copy rate for shared payloads" (today every spawn arg is an unconditional deep clone — Value has no CoW — and nothing measures that cost).

## 3. Empirical probes (%TEMP%\gof-audit, `target\debug\gof.exe`)

| Probe | Program | Result |
|---|---|---|
| a1 | awaited plain task, `1 / 0` | GOF3068 diagnostic, span inside task body, CLI exit 1 — propagated raw, no task context |
| a3 | failing task never joined, main exits immediately | exit 0, prints `7` |
| a3b | failing task never joined, main sleeps 300 ms first | exit 0, prints `7` — **failure silently swallowed** |
| a2 | `go` task, recursion depth 20 / 40 / 100 / 2000 / 100000 | **all abort the process**: exit 0xC00000FD, "thread '<unknown>' has overflowed its stack", no gof diagnostic; depth 10 is fine |
| a2b | `main` recursion depth 2000 / 3000 / 100000 | 2000 OK; 3000 and 100000 abort (0xC00000FD) — budget exceeded is a hard crash even on the 256 MB path |
| b | child task prints, sleeps 400 ms; main returns immediately | `PARENT_RETURNING`, `0`, exit 0 — **child killed before its first print; no leak detection, no warning** |
| c1 | send-after-close | `RuntimeError.ChannelClosed` (Result.Err), no panic, no hang |
| c2 | recv-after-close (empty) | `RuntimeError.ChannelClosed` |
| c3 | buffered(4), 2 sends, close, 3 recvs | `drained:11,22;then:RuntimeError.ChannelClosed` — Go-like drain semantics |
| c4 | rendezvous send blocked, main closes after 100 ms | sender wakes with `Err(ChannelClosed)` — no hang, no lost wakeup |
| d | 10 000 `go inc(i)` + sequential await, sum | completes, correct sum (50005000 % 256 = 8), **23.8 s** in debug; no starvation, no thread-limit failure |
| ex | `examples/task_result_propagation.gof` | `RuntimeError.TaskFailed(message: GOF3068: ...)` — Result-contract boundary works |
| ex | `examples/select_round_robin.gof`, `timeout_cancellation.gof` | both pass |

## 4. RFC-ready problem list (prioritized, slice-shaped)

**P1 — Detached-by-default tasks; no scope, no join-at-exit.** Evidence: probe b; run entry 3278–3365 never waits; RFC 0043:163–165. *Slice*: the structured-concurrency core object (task scope): spawn-within-scope, join-all at scope exit, defined behavior for parent exit and process exit. This is the RFC's central decision; every other problem hangs off it.

**P2 — Unjoined task failures are invisible.** Evidence: probe a3b; outcome stored into a dropped `TaskHandle` (1787–1795) with no observer. *Slice*: exit-time unhandled-failure report (process exit and scope exit): "task `x` failed: GOF3068 …" instead of silence. Trivial once P1 lands; also needs an answer for fire-and-forget use cases.

**P3 — Task-thread stack budget asymmetry.** Evidence: probes a2/a2b — task crashes the process at ~20 frames while main supports ~2000+; pipeline.rs:152 vs spawn_task 6851. *Slice*: uniform recursion budget for spawned tasks (big-stack task threads, or a documented budget + GOF diagnostic instead of 0xC00000FD). Currently a valid program shape (`go` + recursion) hard-crashes the process with no diagnostic.

**P4 — Panic identity is lost across the boundary.** Evidence: `TaskPanic{function_name}` only (1634–1638, 6859–6863); `RuntimeError.TaskPanicked(task: name)` (2348–2354); GOF3010 generic note. *Slice*: panic payload enrichment (message, primary diagnostic code, span) as part of the propagation contract in the RFC.

**P5 — No fan-out API with cancel-on-error.** Evidence: AST only Go/Await; probe d's manual list-append + await loop is the best available pattern; no join_all/scope/nursery symbol anywhere. *Slice*: the RFC's API surface — scope/join semantics, error policy (cancel-siblings vs collect), deadline/token integration with the existing cancellation baseline.

**P6 — Cancellation contract is cooperative-only and half-documented.** Evidence: CancelTokenValue 1103–1173; wake only at wait boundaries; net I/O polls at 25 ms slices (11580–11595); CP-M5-3 `[~]`. *Slice*: finalize the cancellation contract (running-task observation points or explicit cooperative-only doctrine), pin the 25 ms poll floor, define cancel-vs-panic interaction in the RFC.

**P7 — The 3 wake-floor tests are wall-clock flakies in the default suite.** Evidence: 16493, 17200, 17229; BUDGETS.md:96 bans exactly this pattern for gates. *Slice*: convert to a criterion wake-latency bench + deterministic assert (or move behind an `--ignored` gate like `dev_build_budget_gate`), and add the wake-latency measurement Phase 3 requires.

**P8 — Concurrency measurement is one benchmark; no memory/latency model exists.** Evidence: concurrency_runtime.rs (single case); nothing measures per-task memory, spawn/join latency, channel throughput, or RFC 0043:290's boundary deep-copy rate (which the evaluator pays unconditionally — no CoW). *Slice*: concurrency benchmark + stress-gate suite pinned BUDGETS.md-style; this is the "измеренная модель" deliverable and the precondition for RFC 0051's revisit gate.

**P9 — Fairness is unspecified and untested.** Evidence: spec 495 disclaimer; rotation is a thread-local counter (36–51); thread-per-task delegates everything to the OS; M10 item `[~]` with no test. *Slice*: scheduler ADR decision (keep OS threads vs M:N — RFC 0051:116,421 expects a Phase-3 scheduler ADR) backed by tail-latency/starvation stress gates from P8.

**P10 — Unbounded `channel()` has no backpressure.** Evidence: capacity `None` → `can_send` always true (1966–1968, 2152–2156); spec 476. *Slice (small)*: decide the default-channel policy in the RFC (bounded-by-default vs explicit-unbounded) and pin the memory story; feeds P8's measurement plan.

Context notes for the RFC (not problems, but decision inputs): `select` has no timeout arm form (token-armed arms only); `close()` returns `unit` while `send`/`recv` return `Result` (API asymmetry); plain `await` re-raises raw diagnostics while `await_result` wraps them — two shapes for one event; the boundary classification is decided by the spawned function's *declared* return type, which silently changes join semantics.
