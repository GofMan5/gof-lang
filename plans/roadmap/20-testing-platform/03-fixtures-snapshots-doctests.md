# Testing Platform 03: Fixtures, Snapshots, and Doctests

## Goal

Grow the test platform from plain `test fn` into a richer but still typed and
predictable harness.

## Contract

Delivered in slice 1:

- deterministic snapshots under `tests/snapshots/`
- explicit update flow via `--update-snapshots`
- temp files/directories and environment restoration through `TestContext`
- opt-in markdown doctests through `gof test --docs` and fenced ` ```gof doctest ... ` modes
- typed fixtures through `fixture(test) fn` and `fixture(module) fn`
- fixture DAG resolution by parameter name and compatible type

Planned next:

- teardown via `cleanup() -> unit | Result[unit, RuntimeError]`
- doctest extraction from:
  - `README.md`
  - `docs/book/**/*.md`
  - `docs/book-ru/**/*.md`
- unified `tests/ui` and `tests/runtime` product contract under `gof test`
- explicit update flow for `.diag`, `.stdout`, `.stderr`, and `.exit` artifacts

## Non-goals

- global fixture state hidden behind reflection
- snapshot auto-rewrite without user intent
- doctest execution that drifts away from normal compiler/runtime semantics

## Diagnostics Impact

- current fixture delivery adds `GOF3120`, `GOF3121`, `GOF3122`, `GOF3123`,
  and `GOF3124`
- future fixture/doctest slices must still add dedicated diagnostics for cleanup
  misuse and doctest mode misuse instead of overloading generic parse failures

## Tests

- snapshot stability and update behavior
- fixture graph resolution and cycle rejection
- doctest extraction from markdown with line-accurate failure reporting
- UI/runtime fixture diffing with stable path/line/code output

## Exit Criteria

- fixtures, snapshots, and docs examples all live under one coherent harness
- users can teach and test the language through repository docs and normal CLI
