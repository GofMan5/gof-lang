# Contributing to gof

## Non-negotiable rules

1. No feature merges without spec text and executable tests.
2. No undocumented IR invariants.
3. No cross-layer shortcuts between parser, typing, MIR, SSA, and backend.
4. No hidden feature flags or unstable public APIs.
5. No unsafe or FFI additions without a reviewed safe wrapper plan.
6. Every new source file must ship with direct tests, fixture coverage, or both.
7. Deterministic compiler/runtime code targets 100 percent line and branch coverage by policy.
8. Performance regressions are release blockers unless explicitly justified and accepted.
9. Stability, optimization quality, and execution speed take priority over convenience hacks.

## Change process

- Open an RFC for syntax, typing, memory model, concurrency model, ABI, editions, stdlib surface, or package-resolution changes.
- Open an ADR for compiler/runtime architecture, repository layering, backend strategy, GC, scheduler, or diagnostics architecture changes.
- Stable behavior requires:
  - spec text
  - positive and negative tests
  - diagnostics contract updates if errors change
  - benchmark baseline when runtime or performance-sensitive code changes

## Quality gates

- `cargo fmt --all --check`
- `cargo test --workspace`
- `cargo bench -p gof-bench --no-run`

New deterministic compiler/runtime modules are expected to reach full coverage and must not regress existing conformance fixtures or benchmark baselines.
