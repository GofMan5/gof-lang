# RFC 0033: `read_stdin()` and `read_stdin_lines()` baseline

## Summary

Add explicit stdin helpers for script-first workflows:

- `read_stdin() -> Result[string, RuntimeError]`
- `read_stdin_lines() -> Result[list[string], RuntimeError]`

These helpers close a practical automation gap without introducing hidden shell
magic or an implicit streaming model.

## Motivation

`gof` already has explicit file, process, config, JSON, CSV, TOML, and
templating helpers. Without stdin access, shell-driven automation still has to
fall back to temporary files or host-language wrappers.

Python and Node win many internal automation tasks because small programs can
sit naturally inside pipeline-oriented workflows. `gof` needs an equally direct
but more explicit and predictable stdin story.

## Design

- `read_stdin()` reads the full stdin payload and returns it through
  `Result[string, RuntimeError]`
- `read_stdin_lines()` returns `Result[list[string], RuntimeError]`
- both helpers share one cached stdin snapshot per program run
- `read_stdin_lines()` uses the same line splitting semantics as
  `read_lines(path)`
- stdin errors surface as `RuntimeError.Io(message)`

## Non-goals

- streaming stdin iterators
- hidden incremental reads
- separate shell pipeline framework abstractions
- watch-mode-specific stdin orchestration

## Rationale

The cached-snapshot model keeps semantics deterministic:

- repeated calls are stable
- helper behavior does not depend on call ordering
- scripts can derive both raw text and line views from one input payload

This is intentionally narrower than a full stream API, but it is enough for the
current M9 automation target without creating a future compatibility trap.
