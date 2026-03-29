# Testing Platform 03: Fixtures, Snapshots, and Doctests

## Goal

Grow the test platform from plain `test fn` into a richer but still typed and
predictable harness.

## Contract

Delivered in shipped slices:

- deterministic snapshots under `tests/snapshots/`
- explicit update flow via `--update-snapshots`
- temp files/directories and environment restoration through `TestContext`
- opt-in markdown doctests through `gof test --docs` and fenced ` ```gof doctest ... ` modes
- default doctest behavior for plain ` ```gof ` fences when the block looks like a whole-file example with top-level declarations
- ` ```gof ignore ` and ` ```gof text ` opt-outs for plain fences that should stay prose-only
- typed fixtures through `fixture(test) fn` and `fixture(module) fn`
- fixture DAG resolution by parameter name and compatible type
- fixture cleanup hooks through receiver method `cleanup() -> unit | Result[unit, RuntimeError]`
- unified `tests/ui`, `tests/runtime`, and `tests/runtime-fail` product contract under `gof test`
- explicit update flow for `.diag`, `.stdout`, `.stderr`, and `.exit` artifacts
- canonical repo-doc extraction for `README.md`, `docs/site/content/docs/en/**/*.mdx`, and
  `docs/site/content/docs/ru/**/*.mdx` when `gof test --docs` targets the repository root
- dedicated doctest discovery diagnostics for unknown modifiers, conflicting
  execution modes, and unterminated doctest fences

Planned next:

- any broader plain-fence coverage beyond whole-file examples still needs to stay explicit enough that statement-level teaching fragments do not silently turn into unstable docs CI contracts

## Non-goals

- global fixture state hidden behind reflection
- snapshot auto-rewrite without user intent
- doctest execution that drifts away from normal compiler/runtime semantics

## Diagnostics Impact

- current fixture delivery adds `GOF3120`, `GOF3121`, `GOF3122`, `GOF3123`,
  `GOF3124`, and `GOF3125`
- shipped doctest discovery diagnostics now add `GOF3126`, `GOF3127`, and
  `GOF3128` for invalid modifiers, conflicting modes, and unterminated fences

## Tests

- snapshot stability and update behavior
- fixture graph resolution, cycle rejection, and cleanup ordering
- doctest extraction from markdown with line-accurate failure reporting
- UI/runtime fixture diffing with stable path/line/code output

## Exit Criteria

- fixtures, snapshots, and docs examples all live under one coherent harness
- users can teach and test the language through repository docs and normal CLI
