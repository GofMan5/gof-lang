# Testing Platform 05: CI Quality Gates

## Goal

Turn the testing platform into a repository-wide quality gate that scales from
local runs to CI, IDE tooling, and release criteria.

## Contract

Planned deliverables:

- JSON reporter
- JUnit reporter
- stable machine-readable summaries
- dedicated exit codes for test failures vs harness failures
- CI wiring for snapshots, UI/runtime fixtures, property/fuzz/stress, and
  benchmarks
- explicit update workflows that never mutate artifacts implicitly in CI

## Non-goals

- CI-only behavior that differs from local `gof test`
- undocumented output formats
- silent benchmark regressions without stored baselines

## Diagnostics Impact

- reporters must preserve source path, test id, duration, captured output, and
  underlying diagnostics codes/messages

## Tests

- JSON schema regression tests
- JUnit output stability tests
- CI smoke coverage for update flags, failure summaries, and benchmark gate
  thresholds

## Exit Criteria

- CI, editor tooling, and local developer workflows all consume the same honest
  test platform instead of bespoke glue
