# RFC 0040: Hybrid Testing Platform Slice 1

## Summary

Introduce the first shipped user-facing testing surface for `gof`:

- `test fn`
- shipped `testing` stdlib
- hybrid `gof test` discovery for `*_test.gof` and `tests/**/*.gof`
- deterministic snapshots under `tests/snapshots/`

This slice does not claim the full testing platform. It establishes the first
coherent baseline that later fixture, doctest, property, fuzz, stress, and
benchmark work can build on.

## Motivation

`assert(...)` is useful, but it is not a test platform. Without a first-class
testing story, `gof` cannot compete on reliability, repo hygiene, or developer
ergonomics with Python, Go, and Rust.

The first slice must solve three immediate problems:

1. user code needs a typed way to declare tests
2. `gof test` must discover and run those tests deterministically
3. snapshots and per-test helpers must exist without dynamic or reflective
   magic

## Decision

### Language surface

Add `test fn name(...)` as a top-level declaration form.

Rules:

- no receiver methods
- zero parameters or one `t: TestContext`
- return type `unit` or `Result[unit, RuntimeError]`

### Shipped stdlib

Add `stdlib/testing.gof` with:

- `TestContext`
- `TempDir`
- `TempFile`

And the baseline methods:

- `fail`
- `equal`
- `not_equal`
- `true`
- `false`
- `ok`
- `err`
- `match_snapshot`
- `case`
- `temp_dir`
- `temp_file`
- `env`
- `skip`
- `todo`
- `TempDir.path()`
- `TempFile.path()`

### Runner and discovery

`gof test` discovers:

- `*_test.gof`
- `tests/**/*.gof`
- existing diagnostics/runtime fixture targets used by the repository

Snapshots live under `tests/snapshots/` relative to the nearest project root.
They only update when `--update-snapshots` is passed.

## Non-goals

- typed fixtures
- doctests
- JSON/JUnit reporters
- property testing
- fuzzing
- concurrency stress
- benchmark integration
- default parallel execution

These are future slices, not implied by this RFC.

## Diagnostics

This slice adds:

- `GOF3113`: invalid `test fn` contract
- `GOF3114`: invalid operand/contract for `testing` stdlib helpers
- `GOF3115`: unknown requested language-level test
- `GOF3117`: language-level test failure
- `GOF3118`: skipped test
- `GOF3119`: todo test

## Alternatives Considered

### Reuse `assert(...)` only

Rejected. It gives no discovery contract, no snapshots, no per-test resources,
and no durable foundation for richer test categories.

### Build fixtures/doctests/property/fuzz first

Rejected. That would delay the minimum coherent baseline and create too much
surface before basic `gof test` execution is stable.

### Reflection-heavy fixture injection

Rejected. `gof` should not grow a dynamic, order-dependent testing model.

## Test Plan

- lexer/parser/formatter coverage for `test fn`
- typed-HIR validation for valid and invalid test contracts
- interpreter coverage for pass/fail/snapshot/env/temp resource behavior
- CLI regression coverage for discovery, list/filter flow, snapshots, and
  package-aware targets
- docs, examples, roadmap, and VS Code extension synchronization

## Exit Criteria

Slice 1 is complete only when:

- language surface ships
- `gof test` can run real user-facing `test fn`
- snapshots are deterministic and update only explicitly
- docs/spec/roadmap/examples/editor tooling all teach the same contract
