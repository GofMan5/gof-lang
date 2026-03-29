# Testing

`gof` now has the first shipped slice of a real testing platform.

This is not the final test system yet. It is the first coherent baseline that
lets user code and repository code share the same language-level test surface
instead of treating `assert(...)` as a pseudo-framework.

## Current surface

The shipped slice currently includes:

- `test fn`
- `fixture(test) fn` and `fixture(module) fn`
- the reserved stdlib import `testing`
- typed `TestContext`
- deterministic `gof test` discovery for `*_test.gof` and `tests/**/*.gof`
- snapshot storage under `tests/snapshots/`
- artifact-backed product fixtures under `tests/ui`, `tests/runtime`, and `tests/runtime-fail`
- optional fixture cleanup hooks through receiver method `cleanup()`
- markdown doctests through `gof test --docs`, including explicit `gof doctest ...` fences and whole-file plain `gof` examples

Example:

```gof doctest no_run
import testing

test fn sums_are_stable(t: TestContext):
    case = t.case("small-sum")
    case.equal(2 + 3, 5, "expected deterministic integer addition")

test fn result_paths_stay_explicit(t: TestContext) -> Result[unit, RuntimeError]:
    parsed = parse_int("7")?
    return Result.Ok(t.equal(parsed, 7, "expected parse_int to preserve value"))
```

Run it with:

```text
gof test examples/testing_baseline
gof test --list examples/testing_baseline
gof test --shuffle --seed 17 examples/testing_baseline
```

## `TestContext`

The shipped `testing` stdlib currently exposes these baseline helpers:

- `t.fail(message)`
- `t.equal(actual, expected, message)`
- `t.not_equal(actual, expected, message)`
- `t.true(condition, message)`
- `t.false(condition, message)`
- `t.ok(result, message)`
- `t.err(result, message)`
- `t.match_snapshot(name, value)`
- `t.case(name)`
- `t.temp_dir()`
- `t.temp_file(prefix)`
- `t.env(name, value)`
- `t.skip(message)`
- `t.todo(message)`

`TempDir.path()` and `TempFile.path()` give the concrete host paths for the
temporary resources created by the runner.

## Typed fixtures

Fixtures are explicit top-level functions that feed typed dependencies into
tests and other fixtures.

```gof doctest no_run
import testing

fixture(module) fn shared_total() -> int:
    return 41

fixture(test) fn scratch_dir(t: TestContext) -> TempDir:
    return t.temp_dir()

test fn uses_fixtures(shared_total: int, scratch_dir: TempDir, t: TestContext):
    t.equal(shared_total, 41, "expected cached module fixture value")
    t.true(exists(scratch_dir.path()), "expected test fixture temp dir")
```

Current fixture rules:

- `fixture(module)` caches one value per test file during a `gof test` run
- `fixture(test)` creates one value per test case
- dependency injection resolves by parameter name and compatible type
- fixtures must declare an explicit return type and currently return either
  `Type` or `Result[Type, RuntimeError]`
- only `fixture(test)` may request `t: TestContext`
- fixture values may declare an optional receiver method
    `cleanup() -> unit | Result[unit, RuntimeError]`
- test-scoped cleanup runs after the test body but before `TestContext`
    temp/env teardown; module-scoped cleanup runs once after the file finishes

## Fixture cleanup

Returned fixture values may opt into teardown by declaring a receiver method
named `cleanup`.

```gof doctest no_run
import testing

struct ScratchDir:
        path: string

fn ScratchDir.cleanup(self: ScratchDir) -> Result[unit, RuntimeError]:
    return write_file(path_join(self.path, "teardown.log"), "cleanup ran")

fixture(test) fn scratch(t: TestContext) -> ScratchDir:
        dir = t.temp_dir()
        return ScratchDir(dir.path())
```

That keeps fixture lifetime explicit without introducing a second callback DSL:
the returned value owns its own teardown contract, the runner calls it in
reverse dependency order, and test-scoped cleanup still happens before the
runner removes `TestContext` temp resources or restores environment overrides.

## Snapshot contract

Snapshots are explicit and deterministic:

- they live under `tests/snapshots/`
- paths are derived from the source file path and nested `t.case(...)` names
- they update only when you pass `--update-snapshots`

That keeps snapshot churn visible and reviewable instead of letting normal
`gof test` runs rewrite artifacts implicitly.

## Product fixtures

`gof test` now also owns the repository-style product harness paths:

- `tests/ui/*.gof`: the file must fail during compilation
- `tests/runtime/*.gof`: the file must execute successfully
- `tests/runtime-fail/*.gof`: the file must compile, then fail at runtime

Those paths use explicit artifact files:

- `.diag` for diagnostic codes
- `.stdout` for CLI-visible stdout
- `.stderr` for rendered diagnostics
- `.exit` for the expected harness exit status

Update them explicitly with:

```text
gof test --update-snapshots path/to/project
```

That keeps compile/runtime product fixtures under the same CLI entrypoint as
language-level tests instead of splitting the contract across ad hoc scripts.

## JSON reporter

`gof test --json` now emits one machine-readable report to stdout with stable
schema `gof.test.report/v1`.

The report currently includes:

- the normalized input paths and active CLI options
- summary counts plus total wall-clock duration
- per-target events for language tests, compile failures, doctests, fixtures,
  and package targets
- stable ids, source paths, captured stdout/stderr, and structured diagnostics

That lets CI, editor tooling, and other automation consume the same honest test
runner contract instead of screen-scraping the human output.

## JUnit reporter

`gof test --junit` now emits one JUnit/xUnit XML document to stdout.

Current mapping:

- each executed target becomes a `<testcase>` with the stable test id as `name`
- the normalized source path is preserved through `classname` and `file`
- failures render as `<failure>`, skips map to `<skipped>`, and `todo` maps to
    `<skipped type="todo">`
- captured stdout/stderr stay visible through `<system-out>` and `<system-err>`

That gives CI systems a conventional XML path without forcing them to translate
the JSON schema first.

## Exit codes

`gof test` now reserves two dedicated non-zero exit codes:

- `10`: at least one discovered test target failed honestly
- `11`: the test harness itself could not complete discovery, execution, or
    reporting honestly

That lets CI distinguish product regressions from broken invocation or harness
state without parsing stderr heuristically.

## Human output

The default human `gof test` path still keeps passing target output captured so
summary lines stay stable and reviewable.

`gof test --nocapture` now surfaces captured stdout/stderr for language tests,
doctests, fixtures, and package targets on the human runner path instead of
hiding those streams unless a target fails.

## Current limits

This slice is intentionally narrower than the final testing platform.

Not shipped yet:

- automatic doctest execution for every plain `gof` markdown fence, including statement-level prose snippets that are not whole-file examples
- property testing
- fuzzing
- concurrency stress
- benchmark integration
- default parallel execution

Those items are tracked in:

- [`roadmap.md`](../../../roadmap.md)
- [`plans/roadmap/20-testing-platform/00-overview.md`](../../../plans/roadmap/20-testing-platform/00-overview.md)

## Markdown doctests

`gof test --docs` now has a shipped markdown baseline with two entry paths:

- ```` ```gof doctest ````: compile and run
- ```` ```gof doctest no_run ````: compile only
- ```` ```gof doctest compile_fail ````: expect compilation failure
- ```` ```gof doctest runtime_fail ````: expect runtime failure
- plain ```` ```gof ```` fences now default to doctests only when the block looks like a whole-file example that starts with top-level declarations such as `fn`, `import`, `struct`, or `enum`
- ```` ```gof ignore ```` and ```` ```gof text ```` force a plain fence to stay prose-only even when it would otherwise qualify as a whole-file doctest

That keeps the current books honest without pretending every statement-level
teaching fragment is already safe to run as a doctest.

Invalid doctest fences now also fail explicitly instead of being treated as
ambient markdown: unknown modifiers, conflicting execution modes, and
unterminated doctest blocks surface dedicated diagnostics so docs CI does not
silently drift away from the runner contract.

When `gof test --docs` targets the repository root, repo-wide markdown
discovery is now scoped to the canonical teaching sources:

- `README.md`
- `docs/book/src/**/*.md`
- `docs/book-ru/src/**/*.md`

That keeps repository docs coverage focused on the sources of truth instead of
accidentally sweeping arbitrary article drafts or scratch markdown into CI.

`gof test --include-ignored` now opt-ins to discovery inside directories that
the default harness skips on purpose, such as `tests/support/`, `snapshots/`,
`fuzz/`, `corpus/`, or vendor-style markdown trees like `node_modules/`.

That keeps the default contract deterministic and product-oriented while still
giving maintainers an explicit escape hatch for one-off deep validation.

`gof test --shuffle` now opt-ins to deterministic seeded reordering for
language tests, doctests, fixtures, and package targets. `--seed <n>` pins the
active order explicitly, while omitting `--seed` keeps the shuffled order
stable with seed `0` instead of introducing ambient nondeterminism.
