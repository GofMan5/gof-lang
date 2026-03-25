# RFC 0005: stdlib helper baseline

## Summary

Add two explicit bootstrap helpers to the `gof` surface:

- `append(list, value)` to return a new list with one appended item
- `contains(haystack, needle)` for `list` membership and string substring checks

## Motivation

The language already has lists, strings, `len(...)`, and `print(...)`, but real programs still lack basic helper operations that keep code readable without forcing ad hoc loops for every small task.

These helpers move `gof` toward a minimal useful standard library while preserving explicit semantics:

- `append` is pure and returns a fresh list value
- `contains` is pure and allocation-free for strings and list membership checks
- both helpers remain small enough to fit cleanly into the current typed bootstrap pipeline

## Design

### `append(list, value)`

- arity: exactly 2
- first argument must be a list
- second argument must be compatible with the list element type
- return type: the same list element type after compatibility merging
- runtime behavior: clone the list, append one item, return the new list

### `contains(haystack, needle)`

- arity: exactly 2
- supported forms:
  - `contains(list, value) -> bool`
  - `contains(string, string) -> bool`
- return type: `bool`
- runtime behavior:
  - list mode uses equality over supported bootstrap values
  - string mode uses substring lookup

## Diagnostics

- `GOF3039`: invalid operand for builtin `append`
- `GOF3040`: invalid operand for builtin `contains`

## Non-goals

- mutating list helpers
- slicing
- iterators
- regex or advanced string processing
- generic stdlib namespaces before the package and module model matures
