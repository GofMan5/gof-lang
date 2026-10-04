# Concurrency Runtime Measurements (Phase 3, audit P7 + P8)

This file is the concurrency measurement baseline the Phase 3 goals
demand ("measured memory/latency model") and the record for the
wake-latency budget the stress-gate suite enforces. It follows the
discipline of the dev-build budget record
([BUDGETS.md](BUDGETS.md)): canonical fixtures, the measured baseline
on the reference machine, the pin, the tripwire, and the re-measure
procedure.

**Why a separate file and not BUDGETS.md:** BUDGETS.md is the Phase 2
DEV-BUILD budget record — compiler pipeline wall-clock, pinned to the
dev-build fixture, with its own re-pin contract. The concurrency
baseline is a different subject (runtime primitives, thread-per-task
cost model) with different units (rates, per-operation latency) and a
different gate suite. Mixing the two would blur both re-pin
contracts; this record keeps its own table and procedure while
sharing the same shape on purpose.

## The fixtures

All in `fixtures/`, all deterministic and `gof fmt`-clean, all run
through the PUBLIC seam (`compile_source` + `run_with_output`), like
every bench in this crate. The interpreter's
task/channel/cancel-token primitives are private to `gof-compiler`,
so these are end-to-end program runs; each fixture loops the
measured operation enough times per run that interpreter setup
amortizes, and the per-operation number is the run time divided by
the loop count.

| fixture | rounds per run | shape |
| --- | --- | --- |
| `concurrency_spawn_join.gof` | 128 | sequential `go trivial()` + `await`, rendezvous-free trivial task (`return 7`) |
| `concurrency_channel_pairs.gof` | 1024 | buffered (capacity 64) `send`/`recv` pairs, sender and receiver on the same thread |
| `concurrency_rendezvous_wake.gof` | 512 | two tasks ping/pong over a pair of `channel(0)` rendezvous channels — every exchange is a blocked `recv` woken by a `send`, in both directions |
| `concurrency_cancel_wake.gof` | 128 | per round: spawn a waiter blocked on `recv(data, token)`, handshake on a `ready` channel, `cancel(token)`, `await` |
| `concurrency_stress_10k.gof` | 10,000 | spawn 10k tasks, join them all, assert the exact sum (50,005,000) |

The round counts live in `gof_bench::concurrency_runtime`
(`SPAWN_JOIN_ROUNDS`, `CHANNEL_PAIRS`, `RENDEZVOUS_EXCHANGES`,
`CANCEL_WAKE_ROUNDS`, `STRESS_TASKS`) and MUST equal the
`range(...)` bounds baked into the fixtures; the gates compute every
expected value from the same constants, so a fixture edited out of
sync with its constant fails the gate loudly instead of silently
mislabeling a throughput.

## The baseline (the numbers)

Reference machine, measured 2026-10-04: Intel Core i5-10400F @ 2.90
GHz (12 logical processors), 32 GB RAM, Windows 11 Pro 10.0.26200,
rustc 1.93.1 — the same machine BUDGETS.md pins the dev-build budget
on. Measured at HEAD `2616bfd` (which includes the same-day P3 fix
giving every task thread a 256 MB stack — task spawn numbers reflect
that), `cargo bench` (release/bench profile), criterion medians over
100 samples:

| bench id | median per run | rate | per operation |
| --- | --- | --- | --- |
| `runtime blocking rendezvous select` (pre-existing) | 155–205 µs | — | context: one end-to-end 2-task select program, module setup included |
| `concurrency/spawn join round trips` | 13.4–19.8 ms | 6.5–9.6 K round trips/s | **104.7–154.9 µs per go+await round trip** |
| `concurrency/channel send recv pairs` | 1.63–2.01 ms | 509–626 K pairs/s | **1.60–1.97 µs per buffered send/recv pair** |
| `concurrency/rendezvous wake round trips` | 3.11–3.72 ms | 137.6–164.6 K exchanges/s | **6.07–7.27 µs per exchange** (2 wakes) |
| `concurrency/cancel wake round trips` | 19.2–19.8 ms | 6.45–6.65 K rounds/s | **150.3–155.1 µs per cancel-wake round** (spawn + handshake + cancel + wake + join) |

10k stress gate (release profile, `cargo test --release -p gof-bench
--test concurrency_stress_gate -- --ignored --nocapture`): **5.63 s
for 10,000 spawn+join round trips — 1,775 tasks/s, 0.563 ms per
round trip, exact sum (50,005,000) asserted**. The completion assert
is the gate; the wall-clock is context for the thread-per-task cost
model (per-task cost at 10k scale is ~5x the isolated spawn/join
round trip below — more tasks alive at once, more scheduler and
memory pressure). A pre-P3-commit release CLI measured the same 10k
shape at 4.3 s / ~232 µs per task with 2 MB stacks; the audit's debug
measurement was 23.8 s.

Reading of the shape: a spawn+join round trip costs ~105–155 µs —
dominated by the OS thread lifecycle now that task threads reserve
256 MB stacks (the P3 fix), this is the price of thread-per-task and
the honest baseline for any future scheduler decision (audit P9). A
rendezvous wake costs ~6–7 µs — three orders of magnitude below the
25 ms net-I/O polling floor, which is P7's claim, now measured
instead of wall-clock-asserted. A cancel-wake round costs ~150 µs —
spawn/join plus the ready handshake and the wake itself, consistent
with the spawn/join number plus a few wake latencies. Buffered
channel pairs at ~1.6–2.0 µs are pure in-thread mutex+queue work
with no thread handoff — the floor for any future channel
implementation work.

## The pin (wake latency)

The rendezvous wake per-exchange number is the one pinned: five
independent runs measured 6.49, 6.59, 6.07, 6.40, and 7.27 µs —
stable within ±20%, so a budget pin is justified (the same evidence
bar this repo required for the dev-build pin). The pin lives in code
— `gof_bench::concurrency_runtime::wake_budget::RENDEZVOUS_WAKE_EXCHANGE_US`
— so the gate and every future tool read the same number; this file
is the record.

| measurement | bench id | baseline (medians) | BUDGET |
| --- | --- | --- | --- |
| rendezvous wake round trip (per exchange, both directions) | `concurrency/rendezvous wake round trips` | 6.07–7.27 µs | **≤ 10.0 µs** |

The other four measurements are recorded above but deliberately NOT
pinned yet: the spawn/join number includes the just-landed P3 stack
change (re-pin after it settles), the channel and cancel-wake numbers
have only one or two clean full-suite samples each, and the
cancel-wake bench currently trips the intermittent hang documented
below — pinning a number from a bench that can hang would be pinning
an artifact. Each is a named future pin once its bench is stable.

## The gates

```text
cargo test --release -p gof-bench --test concurrency_stress_gate -- --ignored --nocapture
```

- `ten_thousand_spawned_tasks_all_join_and_complete` — the 10k stress
  smoke: completion assert (exact sum) plus a 120 s hang-detection
  ceiling (~30x the measured release number). Not in the default
  suite: the audit measured the same fixture at 23.8 s under the
  debug interpreter.
- `rendezvous_wake_latency_stays_within_budget` — the wake-latency
  tripwire: median of 11 runs after 2 warm-ups, per-exchange, fails
  at 3x the budget (30 µs). Measured on the reference machine:
  median 9.69 µs per exchange (min run 2.98 ms, max run 17.39 ms —
  single-run scheduling outliers exist, which is exactly why the
  gate, like the dev-build gate, compares the MEDIAN). A
  polling-floor regression lands at tens of milliseconds per
  exchange — three orders of magnitude past the tripwire; machine
  noise does not reach 3x (the observed run-to-run spread is ±20%).

Per CONTRIBUTING rule 8, a tripped gate is a release blocker unless
explicitly justified and accepted.

## The hang finding (bench-side watchdog, interpreter-side bug)

The `concurrency/cancel wake round trips` bench intermittently
HANGS the full `cargo bench -p gof-bench --bench
concurrency_runtime` run: 5 of 6 full-suite runs froze during that
bench's warmup or measurement (~300–370 closure calls in, up to ~46k
cancel-wake rounds) with process CPU frozen at a constant value —
every thread blocked in a kernel wait, no failure, no output. The
sixth run (with per-iteration instrumentation) completed all five
benches, and two of the freezes were ended by the watchdog described
below (exit code 2 with the pointer to this section) — the intended
behavior while the interpreter-side bug is outstanding. What was
established while diagnosing (this slice measures and reports; it
does not touch wake behavior):

- NOT a thread-creation failure: the P3 fallback path
  (`spawn_task`'s big-stack spawn failure → panic outcome) was
  instrumented and fired zero times during a hang.
- NOT pure fixture logic: a standalone release harness ran the same
  fixture 600 times clean (with and without a replication of the
  bench's full fixture order — 35k select iterations, 670 spawn/join,
  2400 channel-pair, 1600 rendezvous-wake first); the cancel-wake
  bench alone in a fresh process also runs clean with full criterion
  warmup and measurement.
- The hung process held 15 threads — 12 idle rayon (criterion), the
  criterion main thread, and the waiter task thread — with both the
  main thread and the waiter blocked in futex-class kernel waits.
  A minidump captured at the hang (symbolized with the shipped PDB)
  shows the main thread deep in interpreter evaluation and the waiter
  task thread blocked mid-eval; the raw stack region of the main
  thread contains live return-address frames matching
  `TaskHandle::store` / `WaitSignal::notify` / `Vec::retain` (the
  listener-notification path) interleaved with the blocked wait.
  The dump was preserved at `%TEMP%\gof-p7p8\hung-bench.dmp`
  (ephemeral — recapture by re-running the bench and taking a
  minidump of the frozen `concurrency_runtime-*.exe`).
- Assessment: an intermittent lost wake (or lock starvation) on the
  cancel path that needs thread-scheduling pressure to open — the
  default-suite tests never hit it (they now use a 5 s hang-detection
  timeout, so a recurrence there would be loud), and the clean
  measurement runs show the wake mechanism itself performing at
  µs scale. This is follow-up work for the wake-path owners (audit
  P7/P9/P1 territory), NOT fixed in this slice.

Until it is fixed, the bench carries a watchdog: if a single run of
the cancel-wake bench stops making progress for over 60 s (~3000x
its measured per-iteration time), the bench prints the pointer to
this section and exits with code 2 — a loud fast failure instead of
a silent infinite hang of `cargo bench`.

## Re-measuring and re-pinning

- Re-measure everything:
  `cargo bench -p gof-bench --bench concurrency_runtime`
- Re-measure one surface:
  `cargo bench -p gof-bench --bench concurrency_runtime -- "rendezvous wake"`
- Tripwire check: the gate command above.
- Re-pin (a reviewed decision, never a side effect): change
  `RENDEZVOUS_WAKE_EXCHANGE_US` in
  `gof_bench::concurrency_runtime::wake_budget`, update this record's
  table with the new numbers and date, and say WHY in the change that
  carries it (CONTRIBUTING rule 8: accepted regressions must be
  justified; this file is where acceptance is recorded).
- Changing a fixture's round count: update the matching constant in
  `gof_bench::concurrency_runtime` (the gate's expected values fail
  loudly if you forget), re-measure, and update the table.
