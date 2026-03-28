# RFC 0041: Typed Fixtures and Opt-In Doctests

## Summary

Extend the hybrid `gof` testing platform with:

- typed fixtures through `fixture(test) fn` and `fixture(module) fn`
- fixture dependency resolution by parameter name and compatible type
- opt-in markdown doctests through `gof test --docs` and fenced
  ` ```gof doctest ... ` modes

This RFC does not claim the full testing platform. It closes the next coherent
product slice after RFC 0040 while leaving cleanup hooks, reporters, unified
UI/runtime harnesses, and property/fuzz/stress/bench work for later slices.

## Problem

RFC 0040 shipped a real `test fn` baseline, but it still left two major product
gaps:

1. users had no typed, explicit way to share setup across tests without copy-paste
2. repository docs could not be validated through the normal `gof test` surface

Leaving those gaps open would keep the testing platform honest only for narrow
unit-style tests while examples, setup code, and documentation would continue
to drift.

## Proposed Change

### Typed fixtures

Add a new top-level declaration form:

- `fixture(test) fn name(...) -> Type:`
- `fixture(module) fn name(...) -> Type:`

Rules:

- fixtures are top-level only and cannot declare receivers
- fixtures must declare an explicit scope
- fixtures must declare an explicit return type of either `Type` or
  `Result[Type, RuntimeError]`
- `fixture(test)` may request typed fixture dependencies plus at most one
  `t: TestContext`
- `fixture(module)` may request only typed fixture dependencies
- tests may request zero or more typed fixture dependencies plus at most one
  `t: TestContext`
- dependency resolution is explicit: parameter name selects the fixture and the
  parameter type must be compatible with the fixture value type
- `fixture(module)` values are cached once per language test file during one
  `gof test` run
- `fixture(test)` values are cached once per test case
- fixture graphs must remain acyclic
- broader-lifetime fixtures may not depend on narrower-lifetime fixtures

### Opt-in doctests

Add `gof test --docs` as an explicit markdown doctest path.

Supported fenced modes:

- ```` ```gof doctest ````: compile and run
- ```` ```gof doctest no_run ````: compile only
- ```` ```gof doctest compile_fail ````: expect compilation failure
- ```` ```gof doctest runtime_fail ````: expect runtime failure

This remains opt-in. Plain ` ```gof ` fences do not become executable by
default.

## Alternatives Considered

### Reflection-heavy or type-erased fixtures

Rejected. That would make setup order-dependent, harder to reason about, and
easier to misuse, which conflicts with `gof`'s typed/soundness-first direction.

### Scope-free fixtures with implicit lifetime

Rejected. Fixture lifetime changes runtime behavior and cache visibility, so it
must stay explicit in the declaration surface.

### Always-on doctests for every `gof` markdown fence

Rejected for now. The repo still contains teaching fragments that are useful as
documentation but are not all safe to execute as product doctests. Opt-in modes
keep the contract honest.

## Compatibility Impact

- adds new top-level syntax `fixture(scope) fn`
- expands the valid `test fn` parameter contract from `{}` or one
  `TestContext` parameter to typed fixture injection plus at most one
  `TestContext`
- adds `gof test --docs` as a new CLI behavior without changing default
  discovery
- keeps existing `test fn` code valid
- keeps plain markdown `gof` fences non-executable unless explicitly marked

## Diagnostics Impact

This RFC adds:

- `GOF3120`: invalid `fixture(scope) fn` contract
- `GOF3121`: missing or unknown fixture scope
- `GOF3122`: fixture dependency cycle
- `GOF3123`: unresolved or incompatible typed fixture dependency
- `GOF3124`: invalid fixture lifetime dependency between scopes

Existing `GOF3113` remains the diagnostic for invalid `test fn` contracts.

## Test Plan

- lexer/parser/formatter tests for `fixture(scope) fn`
- typed-HIR tests for valid fixtures, invalid scopes, missing return types,
  unresolved dependencies, lifetime violations, and cycles
- interpreter tests for test-scoped fixtures, module-scoped fixture caching,
  and `Result.Err(...)` fixture failures
- CLI conformance tests for list/run flow, typed fixture resolution, and
  markdown doctest execution
- docs/example/editor synchronization tests, including VS Code grammar/snippet
  coverage for fixture syntax

## Benchmark Impact

No dedicated performance gate is introduced in this RFC. Fixture resolution
adds test-runner work, but the current implementation is intentionally small:
module-scoped fixtures cache once per test file and test-scoped fixtures cache
once per test case. Wider benchmark/stress gates remain future testing-platform
slices.
