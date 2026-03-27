# RFC 0011: Conversion Helper Baseline

## Summary

Add a minimal explicit conversion slice:

- `parse_int(text)`
- `to_string(value)`

## Motivation

`gof` now has strings, file I/O, list and dict helpers, and baseline text work.
Without explicit conversion helpers, small CLI-style and text-processing programs
still need awkward workarounds whenever numeric data crosses a string boundary.

## Design

- `parse_int(text)` accepts one string and returns `Result[int, RuntimeError]`
- invalid numeric text becomes `Result.Err(RuntimeError.ParseInt(message))`
- `to_string(value)` accepts one printable value and returns `string`
- conversion is explicit; the language still avoids implicit coercion rules

## Diagnostics

- `GOF3060`: invalid operand for `parse_int`
- `GOF3062`: invalid operand for `to_string`

## Non-goals

- float parsing
- locale-aware formatting
- arbitrary radix parsing
- automatic coercions in binary operators or calls
