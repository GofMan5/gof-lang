# ADR 0004: Backend Strategy

## Status

Accepted

## Decision

Dual-profile code generation from the backend IR (a serializable linear
SSA form, CLIF / LLVM IR class — not a stack-VM bytecode):

- **All eight pipeline stages from ADR 0002 run in both profiles.** The
  profiles differ in pass depth, never in stage skipping: dev builds MIR
  and SSA with shallow passes (MIR construction is required anyway for
  `async` state-machine lowering) and lowers to backend IR with minimal
  passes; release runs the full SSA optimization pipeline.
- **Dev profile — Cranelift.** Pure-Rust toolchain, no LLVM C dependency.
  Per-module parallel codegen, no redundant intermediate serializations.
  Serialized IR is the incremental cache boundary, stored in a compact
  binary zero-copy format (rkyv class), never textual dumps. The existing
  `bootstrap-ir-v0` JSON artifact is transitional and retires when the
  binary backend IR cache lands.
- **Generic functions in dev dispatch through witness tables** (type-erased)
  with instance caching at module boundaries; full monomorphization runs
  only in the release profile so generics can never eat the dev compile
  budget.
- **Release profile — LLVM path behind a feature flag.** Trigger: the
  Cranelift release profile trailing the Rust-class baseline on the
  Phase 6 scorecard workloads by a margin that matters (>15% on two or
  more workloads). The first real native artifact ships Cranelift-only.

Validation gates (Phase 2 kickoff, benchmark-pinned, not assumed):

- dev wall-clock budget for a small rebuild is pinned by a workspace-scale
  benchmark at Phase 2 start (Go-class target)
- release throughput compares dev profile vs release profile on scorecard
  workloads; the evaluator is never a performance baseline

## Consequences

- dev builds stay fast by construction: all stages present but shallow,
  parallel module codegen, binary IR cache, no monomorphization explosion
- release optimization work is deferred until real artifacts exist, and
  its trigger is measurable instead of circular
- the bootstrap evaluator remains the fallback execution path until the
  Cranelift artifact passes the smoke gates, then it is demoted to
  conformance-oracle status
- adding LLVM later is additive (feature flag), not a migration
- ADR 0002 stays intact: layering is about one-way stage ordering, and
  both profiles honor it fully
