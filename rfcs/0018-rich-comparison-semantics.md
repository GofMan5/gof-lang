# RFC 0018: Rich Comparison Semantics

## problem

The language already exposes comparison syntax, but the contract has been too narrow and too implicit:

- ordering was effectively limited to `int`
- string comparison had equality but not ordering
- structural equality existed for some aggregates but was not specified as a coherent rule
- nested values could accidentally drift toward identity-style equality through containers or payload types

That makes the data model harder to reason about and leaves an unfinished gap in M3.

## proposed change

Define comparison as two distinct contracts:

- equality through `==` and `!=`
- ordering through `<`, `<=`, `>`, and `>=`

Ordering remains intentionally narrow and predictable:

- `int`
- `string` with lexicographic ordering

Equality becomes explicit structural/value semantics for deterministic types:

- `int`
- `string`
- `bool`
- `json`
- `unit`
- `list[T]` when `T` is equality-comparable
- `dict[T]` when `T` is equality-comparable
- same-type `struct` when every field is equality-comparable
- same-type `enum` when every payload field is equality-comparable
- `Result[T, E]` when both `T` and `E` are equality-comparable

Non-goals for this RFC:

- no ordering for lists, dicts, structs, enums, or results
- no identity equality for channels, tasks, or cancellation tokens
- no implicit fallback from unsupported comparisons to runtime surprises

The type layer now rejects unsupported equality and ordering with `GOF3087`.

## alternatives considered

- Allow identity equality for channels, tasks, and cancellation tokens.
  This was rejected because it makes equality partly structural and partly pointer-based in a way that is hard to teach and easy to misuse.
- Allow ordering for lists or structs.
  This was rejected because lexicographic or declaration-order comparisons would add surface area without a strong semantic payoff and would complicate future evolution.
- Keep string ordering out of the language.
  This was rejected because string ordering is operationally useful, deterministic, and materially simpler than adding user-defined comparison hooks.

## compatibility impact

This is a strictly additive improvement for well-typed programs that already relied on deterministic data comparisons.

Programs that attempted unsupported comparisons may now fail earlier and more precisely with `GOF3087` instead of drifting into generic bootstrap-evaluator failures.

## diagnostics impact

Add `GOF3087` for invalid equality or ordering operands.

Expected diagnostic behavior:

- mixed-type equality such as `1 == "1"` is rejected in the type layer
- ordering on non-ordered values such as `[1, 2] < [1, 2]` is rejected in the type layer
- structural equality is rejected when nested fields or payloads contain non-comparable values such as channels, tasks, or cancellation tokens

## test plan

- typed HIR unit tests for valid and invalid comparison contracts
- evaluator tests for lexicographic string ordering plus structural equality over json, unit, enums, lists, dicts, structs, and results
- pipeline coverage to ensure comparison semantics survive lowering through SSA
- pass fixtures and fail fixtures for the public language surface
- runnable example plus CLI test coverage
- docs/spec/roadmap synchronization checks through repository tests and book builds

## benchmark impact

No dedicated benchmark gate is required for this step because the change does not add a new hot path algorithm.

The comparison rules are still important for future optimization work because they keep the cost model explicit:

- ordering remains scalar-only
- equality remains structural only for deterministic data
- no hidden runtime dispatch or dynamic comparator registration is introduced
