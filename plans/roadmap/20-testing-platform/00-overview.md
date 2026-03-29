# Testing Platform Overview

## Goal

Build a first-class testing platform for `gof` that combines the best practical
properties of Python, Go, and Rust:

- language-level tests
- deterministic discovery
- strong typed contracts
- snapshots, fixtures, doctests, property/fuzz/stress, and benchmarks
- CI-friendly output and quality gates

## Current Delivered Surface

Slice 1 is shipped, and the next testing slice now has its first delivered
pieces:

- `test fn`
- shipped `testing` stdlib baseline
- hybrid `gof test` discovery for `*_test.gof`, `tests/**/*.gof`, and existing
  repo fixtures
- snapshot baseline under `tests/snapshots/`
- list/filter/fail-fast/nocapture/update-snapshots flow
- typed fixtures through `fixture(test) fn` and `fixture(module) fn`
- opt-in markdown doctests through `gof test --docs`

## Program Order

1. language surface
2. runner, discovery, and CLI
3. fixtures, snapshots, and doctests
4. property, fuzz, stress, and bench
5. CI quality gates and reporting

## Non-goals

- monkeypatching
- reflection-driven fixture magic
- hidden dependency overrides
- non-deterministic default execution

## Diagnostics Impact

- slice 1 introduces `GOF3113`, `GOF3114`, `GOF3115`, `GOF3117`, `GOF3118`,
  and `GOF3119`
- later shipped fixture and doctest slices extend that set through `GOF3120`-
  `GOF3128`
- future slices must reserve new diagnostics explicitly instead of overloading
  generic compiler/runtime codes

## Tests

- every slice must add unit, integration, diagnostics, and regression coverage
- the legacy Rust-level harness remains a lower quality gate, not a competitor
  to the new user-facing test platform

## Exit Criteria

- `gof` has one coherent testing story instead of separate ad hoc harnesses
- the testing story is teachable from repo docs without reading compiler code
