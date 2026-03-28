# RFC 0034: `unix_seconds()` and `unix_millis()` baseline

## Summary

Add explicit wall-clock helpers for script-first workflows:

- `unix_seconds() -> Result[int, RuntimeError]`
- `unix_millis() -> Result[int, RuntimeError]`

These helpers close a practical automation gap without committing `gof` to a
full datetime or timezone surface too early.

## Motivation

Internal automation scripts routinely need current time for:

- log stamps
- retry windows
- polling cursors
- temp-name suffixes
- freshness checks against external systems

Without explicit time helpers, `gof` scripts have to shell out to host tools or
delegate simple timing logic to another language. That hurts the script-first
story even though the required semantics are narrow and well understood.

## Design

- `unix_seconds()` returns the current Unix wall clock in whole seconds through
  `Result[int, RuntimeError]`
- `unix_millis()` returns the current Unix wall clock in whole milliseconds
  through `Result[int, RuntimeError]`
- both helpers accept zero arguments
- host clock failures are surfaced as `RuntimeError.Time(message)`
- clocks before the Unix epoch are rejected explicitly
- timestamps that exceed the current bootstrap `int` range are rejected
  explicitly

## Non-goals

- a first-class datetime type
- timezone APIs
- human-readable formatting helpers
- calendar arithmetic
- scheduling framework abstractions

## Rationale

This is the narrowest useful time surface for the current M9 automation target.
It is enough for real scripts while keeping future datetime design space open.

Using explicit `Result` values also keeps the contract aligned with the rest of
the current stdlib baseline:

- no hidden host failures
- no implicit fallback clocks
- no silent coercion into string formatting
