# RFC 0042: Fixture Cleanup Hooks

## Summary

Extend typed fixtures with an explicit teardown protocol:

- fixture return values may declare receiver method `cleanup()`
- supported cleanup signatures are `-> unit` and
  `-> Result[unit, RuntimeError]`
- `gof test` calls cleanup hooks in reverse dependency order
- test-scoped cleanup runs before `TestContext` temp/env teardown
- module-scoped cleanup runs once after the language test file finishes

This RFC closes the next coherent testing-platform slice after RFC 0041 without
claiming reporters, property/fuzz/stress, or broader fixture lifetimes.

## Problem

RFC 0041 shipped typed fixtures and caching semantics, but it still left one
important lifecycle gap: setup could be shared explicitly, while teardown still
had to be encoded indirectly through `TestContext` temp resources or ad hoc
helper calls inside tests.

That made long-lived fixture values harder to manage and kept fixture lifetime
discipline incomplete.

## Proposed Change

Returned fixture values may opt into teardown by declaring a receiver method on
their value type:

- `fn Type.cleanup(self: Type):`
- `fn Type.cleanup(self: Type) -> Result[unit, RuntimeError]:`

Rules:

- cleanup hooks are optional
- cleanup hooks take no explicit parameters beyond the receiver
- cleanup hooks may currently target fixture value types with method-capable
  receivers, namely structs and shipped opaque runtime types
- test-scoped fixtures clean up after the test body completes and before the
  runner removes `TestContext` temp resources or restores environment overrides
- module-scoped fixtures clean up once after the file-level test run completes
- cleanup order is reverse creation order, which also preserves reverse
  dependency order for fixture graphs
- cleanup failures fail the affected test run instead of being dropped silently

## Alternatives Considered

### Separate `teardown fn` declarations

Rejected. That would duplicate fixture identity and lifetime information across
multiple declarations while making dependency order harder to reason about.

### Hidden cleanup through reflection or naming conventions

Rejected. The cleanup contract is lifecycle-relevant behavior, so it must stay
typed and explicit instead of being inferred through dynamic lookup.

### Keep teardown limited to `TestContext`

Rejected. `TestContext`-managed temp/env helpers are useful, but they do not
cover arbitrary fixture-owned values and they are too narrow to express a real
fixture lifecycle contract.

## Compatibility Impact

- keeps existing fixtures valid because cleanup hooks are optional
- adds no new top-level syntax
- extends the runtime behavior of `gof test` so fixture values may now own
  explicit teardown
- preserves existing `TestContext` cleanup behavior and orders fixture cleanup
  before it for test-scoped values

## Diagnostics Impact

This RFC adds:

- `GOF3125`: invalid fixture cleanup hook contract

`GOF3125` covers cleanup misuse such as extra parameters or unsupported return
types.

## Test Plan

- typed-HIR validation for invalid cleanup hook arity and return types
- interpreter tests for test-scoped cleanup ordering, module-scoped cleanup, and
  cleanup failures
- CLI conformance tests for end-to-end cleanup execution and cleanup-triggered
  test failures
- docs/spec synchronization updates for the shipped cleanup contract

## Benchmark Impact

No dedicated benchmark gate is introduced in this RFC. Cleanup adds bounded
runner work proportional to the number of resolved fixtures in a test file and
remains deterministic by construction.