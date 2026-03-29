# Testing Platform 05: CI Quality Gates

## Goal

Turn the testing platform into a repository-wide quality gate that scales from
local runs to CI, IDE tooling, and release criteria.

## Contract

Delivered in this slice:

- JSON reporter through `gof test --json`
- JUnit reporter through `gof test --junit`
- dedicated exit codes: `10` for discovered target failures and `11` for
  harness failures
- stable machine-readable report schema `gof.test.report/v1`
- summary counts, per-target events, captured stdout/stderr, and structured
  diagnostics in one stdout payload
- JUnit/xUnit XML output with testcase ids, source paths, durations, and
  failure or skipped markers

Planned remaining deliverables:

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
