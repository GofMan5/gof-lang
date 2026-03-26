# RFC 0012: Range Baseline

## Summary

Add an explicit integer sequence builder:

- `range(stop)`
- `range(start, stop)`
- `range(start, stop, step)`

## Motivation

`gof` already supports `for ... in ...` over lists, strings, and dict keys. What
it still lacks is a clean way to generate integer sequences without writing
manual counters for every loop.

`range` raises the usefulness of loops immediately while staying explicit about
allocation and sequence shape.

## Design

- `range(stop)` expands to integers from `0` up to but excluding `stop`
- `range(start, stop)` expands from `start` toward `stop` with step `1`
- `range(start, stop, step)` uses an explicit non-zero step
- the result is always a concrete `list[int]`
- no lazy iterator or hidden view semantics are introduced in this baseline

## Diagnostics

- `GOF3063`: invalid operand for `range`
- `GOF3064`: zero-step contract violation for `range`

## Non-goals

- float ranges
- lazy ranges
- open-ended ranges
- implicit coercion from string or bool to int
