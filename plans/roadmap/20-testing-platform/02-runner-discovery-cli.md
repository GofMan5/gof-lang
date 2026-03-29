# Testing Platform 02: Runner, Discovery, and CLI

## Goal

Turn `gof test` into the canonical test entrypoint for user code and repo-level
quality gates.

## Contract

Delivered in shipped slices:

- deterministic discovery for:
  - `*_test.gof`
  - `tests/**/*.gof`
  - existing repo fixtures/runtime harness paths
- `gof test [paths...]`
- `--filter`
- `--exact`
- `--list`
- `--fail-fast`
- `--nocapture`
- `--update-snapshots`
- `--docs`
- `--include-ignored`
- `--json`
- `--junit`
- per-test stdout capture
- deterministic display ids based on normalized source paths

Planned later:

- `--jobs`
- `--shuffle`
- `--seed`
- `--timeout-ms`
- `--stress`
- dedicated `gof fuzz`
- full `gof bench` integration

## Non-goals

- silently changing semantics between one-shot and test-runner execution
- magic fallback to external harnesses
- non-deterministic ordering by default

## Diagnostics Impact

- runner output must preserve underlying compiler/runtime diagnostics instead of
  flattening them into generic harness text

## Tests

- CLI regression coverage for discovery, list, filtering, invalid contracts,
  snapshots, and package-aware targets
- path normalization coverage for package roots and file targets

## Exit Criteria

- `gof test` is useful for both repo work and user code
- discovery and summary output are stable enough to build richer reporters on top
