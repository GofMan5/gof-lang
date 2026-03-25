# Contributing to gof

## Non-negotiable rules

1. No feature merges without spec text and executable tests.
2. No undocumented IR invariants.
3. No cross-layer shortcuts between parser, typing, MIR, SSA, and backend.
4. No hidden feature flags or unstable public APIs.
5. No unsafe or FFI additions without a reviewed safe wrapper plan.

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

New deterministic compiler/runtime modules should target full branch coverage and must not regress existing conformance fixtures.
