# Dev-Build Budget (Phase 2, benchmark-pinned)

ADR 0004 requires the dev-build wall-clock budget to be "pinned by a
benchmark at Phase 2 start, not assumed" (the Phase 2 exit criterion
"бюджеты записаны бенчмарками"). This file is that pin: the canonical
program, the measured baseline on the reference machine, the budget
ceilings, and the tripwire that guards them.

## The canonical fixture

`fixtures/dev_budget.gof` — 90 lines, eleven functions, int/bool only:
nested `while` loops, `if`/`else` chains, calls with bool arguments,
`and`/`or`/`not` logic, the full int operator set. Exactly the v1
machine-code surface (`gof build --native-object`); `gof fmt`-clean;
deterministic by construction — every value is a pure function of the
literals, `main` returns **163** (the evaluator and the native binary
agree), and the emitted object is byte-identical between cached and
uncached builds (1945 bytes).

## The budget (the pin)

Measured through the public pipeline seams, `cargo bench` (release)
profile, **excluding the link step** (rationale below). The budget
constants live in code — `gof_bench::dev_build_budget` — so the gate
and every future tool read the same numbers; this file is the record.

| measurement                                       | bench id                            | baseline (median) | BUDGET |
| ------------------------------------------------- | ------------------------------------ | ----------------- | ------ |
| uncached dev-build pipeline (frontend → IR → object emission) | `dev build pipeline uncached` | 3.0 ms  | **≤ 4.0 ms** |
| cache-hit rebuild pipeline (graph load → key → decode → object emission) | `dev build pipeline cache hit` | 2.6 ms | **≤ 3.0 ms** |
| frontend only (parse → SSA; what the cache skips)  | `dev build frontend parse to ssa`   | 1.0 ms            | diagnostic, not pinned |

Reference machine, measured 2026-10-04: Intel Core i5-10400F @ 2.90
GHz, 32 GB RAM, Windows 11 Pro 10.0.26200, rustc 1.93.1, rust-lld.

The current shape is worth reading before optimizing: object emission
(Cranelift codegen) dominates (~2.2 ms of the ~3 ms uncached build);
the frontend is ~1 ms, which is why the IR cache saves ~0.5 ms
in-process on this fixture. The pipeline's own work is already
Go-class for programs of this size — the wall-clock problem lives
after it (next section).

## Why the link step is excluded from the pinned budget

`gof build --native-object` links by spawning external processes, and
`probe_linkers()` itself shells out to `rustc --print sysroot` before
spawning the linker. That is machine state, not compiler state — the
cache contract draws the same line (a no-linker machine still
populates the cache). Concretely, on the reference machine the
link-inclusive numbers were:

- full uncached build: 1.2–1.9 s per build (criterion and release-CLI
  wall-clock), with run-to-run swings larger than the ENTIRE in-process
  pipeline
- full cache-hit build: ~1.4–1.7 s
- repeated linker spawning intermittently hard-crashed the bench
  binary (antivirus/process interference, silent exit) — unusable
  inside the `cargo bench` quality gate

Pinning those numbers would pin the machine's linker and antivirus,
not the compiler. So: **the budget pins the compiler's own work; the
link-inclusive experience is context** (below) and a named future
target — linker-probe caching and cheaper linking are the actual
wall-clock lever for dev builds today.

## Measured full-build experience (context, NOT pinned)

Release CLI, reference machine, the canonical fixture
(`gof build <fixture> --native-object <out>`, 3072-byte exe,
1945-byte object, exit code 163):

- uncached full build: ~1.8 s wall
- `--cache` first build (miss): ~1.9 s wall
- `--cache` second build (hit): ~1.5 s wall

Re-measure on any machine (no gate, just a reading):

```text
cargo build --release -p gof-cli
$target\release\gof.exe build benchmarks\gof-bench\fixtures\dev_budget.gof --native-object %TEMP%\dev_budget.exe
$target\release\gof.exe build ... --cache   (first = miss, second = hit)
```

## The gate (tripwire, not a precision gate)

```text
cargo test --release -p gof-bench --test dev_build_budget_gate -- --ignored --nocapture
```

Re-measures the canonical fixture's dev-build pipeline (median of 15
runs after warm-up) through the SAME lib helpers the benchmark uses
and fails loudly when the median exceeds **3× the budget** —
12 ms uncached, 9 ms cache-hit. The margin is deliberately generous:
3× absorbs machine noise while still catching the 10×-class
regressions that actually eat a dev budget. The tests are `#[ignore]`d
— wall-clock assertions never run in the default test suite — and the
gate refuses debug-profile runs with instructions (the budget is a
release-profile number). Per CONTRIBUTING rule 8, a tripped gate is a
release blocker unless explicitly justified and accepted.

## Re-measuring and re-pinning

- Re-measure: `cargo bench -p gof-bench -- "dev build"`
- Tripwire check: the gate command above
- Re-pin (a reviewed decision, never a side effect): change the
  constants in `gof_bench::dev_build_budget`, update this record's
  table with the new numbers and date, and say WHY in the change that
  carries it (CONTRIBUTING rule 8: accepted regressions must be
  justified; the budget file is where acceptance is recorded).
