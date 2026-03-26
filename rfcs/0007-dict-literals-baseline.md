# RFC 0007: Dict Literals Baseline

## Summary

Add bootstrap dict literal syntax:

```gof
metrics: dict = {"ok": 7, "warn": 2}
```

This is sugar for constructing a dict value directly, without weakening the
language rule that key/value operations must stay explicit and type-checkable.

## Motivation

The language already supports dict values through `dict()` and `insert(...)`.
That is honest, but verbose for the common case where the whole dictionary is
known at construction time.

Without literal syntax, users learn the wrong lesson:

- dicts look more awkward than lists
- simple examples spend too much surface area on construction mechanics
- docs and examples feel less expressive than the rest of the language

Literal syntax improves readability without adding hidden mutation.

## Design

- Syntax: `{"key": value, "other": value}`
- Empty dicts are allowed through `{}`
- Keys currently must resolve to `string`
- Values currently must resolve to one compatible type when the bootstrap type
  layer can determine them
- Dict literals lower through HIR, typed HIR, MIR, SSA, and the evaluator as
  first-class expressions

## Non-goals

- non-string keys
- generic dict surface syntax
- ordered map guarantees beyond the current deterministic bootstrap behavior
- hidden in-place mutation

## Diagnostics

- `GOF3049`: invalid dict literal key type
- `GOF3050`: incompatible dict literal value types

## Teaching impact

The book and examples should teach dict literals as the default way to express a
fully known dictionary. `dict()` plus `insert(...)` remain the explicit path
when construction must happen incrementally.
