# RFC 0003: Match Baseline

## Summary

Add statement-level `match` over unit enum variants with an exhaustiveness baseline.

## Motivation

`gof` can already model domain state through `enum`, but without explicit branching over
those states the language forces users back into chains of equality checks. That is weaker
than the design target and too easy to grow into ad hoc control flow.

## Scope

This RFC intentionally keeps the first `match` slice narrow:

- statement-level `match`
- enum-only targets
- unit variant patterns written as `EnumName.Variant`
- no wildcard arm
- no payload variants
- no expression-form `match`

## Syntax

```gof
match status:
    Status.Ready:
        return 1
    Status.Busy:
        return 2
```

## Static Rules

- the `match` target must resolve to a known enum type
- each arm pattern must be a variant of that same enum
- duplicate variants inside one `match` are rejected
- every declared unit variant on the enum must be covered

## Diagnostics

- `GOF3030`: duplicate match arm
- `GOF3031`: invalid match arm pattern or wrong enum
- `GOF3032`: non-enum `match` target
- `GOF3033`: non-exhaustive `match`

## Non-Goals

- payload variants
- destructuring
- wildcard or guard arms
- lowering to public stable backend control-flow semantics
