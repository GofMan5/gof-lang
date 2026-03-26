# RFC 0010: String Helper Baseline

## Summary

Add a small, explicit bootstrap string-helper slice:

- `trim(text)`
- `split(text, separator)`
- `join(parts, separator)`
- `starts_with(text, prefix)`
- `ends_with(text, suffix)`

## Motivation

`gof` can already do basic output, file I/O, lists, dicts, and control flow, but
it still lacks the minimal text-processing surface required for honest CLI-style
programs. Users should not need compiler-internal tricks just to normalize input,
split a line, join parts, or test prefixes and suffixes.

## Design

- helpers stay explicit function calls
- no hidden mutation
- no hidden view semantics
- return types stay predictable:
  - `trim(string) -> string`
  - `split(string, string) -> list[string]`
  - `join(list[string], string) -> string`
  - `starts_with(string, string) -> bool`
  - `ends_with(string, string) -> bool`
- `split` rejects an empty separator in the bootstrap contract to avoid teaching
  ambiguous string behavior too early

## Diagnostics

- `GOF3055`: invalid operand or contract for `split`
- `GOF3056`: invalid operand for `join`
- `GOF3057`: invalid operand for `trim`
- `GOF3058`: invalid operand for `starts_with`
- `GOF3059`: invalid operand for `ends_with`

## Non-goals

- regex support
- unicode-normalization helpers
- advanced formatting APIs
- hidden iterator/view string slices
