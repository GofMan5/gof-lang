# ADR 0005: Repository Boundaries

## Status

Accepted

## Decision

- `compiler/gof-compiler` owns the layered compiler: lexer → CST → AST →
  HIR → typed HIR → MIR → SSA → backend IR, plus the bootstrap evaluator.
  The evaluator is a temporary resident: it demotes to a conformance
  oracle once the Cranelift artifact passes its smoke gates (ADR 0004).
- `runtime/gof-runtime` owns runtime contracts shared by evaluator and
  native target (task, channel, and cancellation primitives).
- `tools/gof-cli` owns the CLI surface only; no language semantics live
  there.
- `stdlib/` ships `.gof` modules. Rust builtin implementations are allowed
  only for host-IO/network primitives that cannot yet be expressed in
  `.gof`, and every such builtin documents a migration exit toward `.gof`.
- `tests/` owns fixtures, snapshots, and the conformance crate;
  `benchmarks/` owns the benchmark harness.
- `VISION.md`, `roadmap.md`, `spec/`, `rfcs/`, `adrs/`, `plans/` own
  contracts and the decision trace: `VISION.md` is the vision-level
  program, `roadmap.md` the status index, RFCs the public-behavior trace,
  ADRs the architecture trace.

## Consequences

- language semantics never leak into the CLI crate
- Rust-side stdlib surface stays bounded and carries explicit migration
  exits, keeping the eventual native target free of hidden host
  dependencies
- the evaluator's residence in the compiler crate is time-bounded by the
  ADR 0004 smoke gates instead of becoming permanent
