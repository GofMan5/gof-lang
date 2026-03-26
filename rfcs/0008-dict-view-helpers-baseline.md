# RFC 0008: Dict View Helpers Baseline

## Summary

Add bootstrap dict view helpers:

```gof
names = keys(metrics)
counts = values(metrics)
```

These helpers expose deterministic dict traversal without adding hidden
mutation, hidden sorting contracts, or tuple syntax the language does not have
yet.

## Motivation

The dict baseline already supports literals, indexing, membership checks, and
incremental construction. What it still lacked was a clear, explicit way to
turn a dict into data that can be iterated, indexed, and validated in normal
program logic.

Without `keys(...)` and `values(...)`, users are forced into either:

- direct dict iteration when they only need keys
- repeated dict indexing when they need a stable view of both names and values

That is workable, but it under-teaches how deterministic dict traversal should
look in `gof`.

## Design

- `keys(dict)` returns `list[string]`
- `values(dict)` returns `list[value_type]`
- both helpers currently require exactly one dict argument
- both helpers preserve the runtime's deterministic key ordering
- `values(dict)` returns values in the same order as `keys(dict)`

## Non-goals

- tuple-based `items(...)`
- non-string dict keys
- lazy iterators
- hidden in-place mutation

## Diagnostics

- `GOF3051`: invalid operand for builtin `keys`
- `GOF3052`: invalid operand for builtin `values`

## Teaching impact

The book and examples should teach `keys(...)` and `values(...)` as explicit
view construction. They are not magical references into a mutable dictionary;
they are concrete list values that fit the rest of the bootstrap language.
