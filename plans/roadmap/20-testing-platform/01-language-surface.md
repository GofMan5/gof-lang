# Testing Platform 01: Language Surface

## Goal

Define language-owned test declarations and typed testing helpers without
turning `gof` into a dynamic test DSL.

## Contract

Slice 1 shipped:

- `test fn name(...)`
- top-level only
- zero parameters or one `t: TestContext`
- return type `unit` or `Result[unit, RuntimeError]`
- shipped `testing` stdlib with:
  - `TestContext`
  - `TempDir`
  - `TempFile`
  - `fail`, `equal`, `not_equal`, `true`, `false`, `ok`, `err`
  - `match_snapshot`, `case`, `temp_dir`, `temp_file`, `env`, `skip`, `todo`
  - `TempDir.path()` and `TempFile.path()`

Delivered in the next testing slice:

- `fixture(test) fn name(...)`
- `fixture(module) fn name(...)`
- test parameters may resolve fixtures by parameter name and compatible type
- fixtures require explicit return types of `Type` or
  `Result[Type, RuntimeError]`
- only `fixture(test)` may request `t: TestContext`

Planned later:

- `bench fn`
- `property fn`
- `fuzz fn`
- `stress fn`
- fixture lifetime scopes and cleanup protocol

## Non-goals

- first-class callback-heavy test DSL
- hidden fixture injection through import rewriting
- per-test monkeypatch hooks

## Diagnostics Impact

- `GOF3113`: invalid `test fn` contract
- `GOF3114`: invalid operand/contract for shipped `testing` helpers
- `GOF3115`: unknown requested language-level test
- `GOF3117`: runtime test failure
- `GOF3118`: skipped test
- `GOF3119`: todo test
- `GOF3120`: invalid fixture contract
- `GOF3121`: missing or unknown fixture scope
- `GOF3122`: fixture dependency cycle
- `GOF3123`: unresolved or incompatible typed fixture dependency
- `GOF3124`: invalid fixture lifetime dependency between scopes

## Tests

- lexer/parser/formatter coverage for `test fn`
- lexer/parser/formatter coverage for `fixture(scope) fn`
- typed-HIR validation for valid and invalid test contracts
- typed-HIR validation for fixture scopes, dependency resolution, and cycles
- runtime coverage for assertions, snapshots, temp resources, env restore,
  skip/todo control flow, and fixture caching

## Exit Criteria

- the language can express real unit-style tests without relying on `assert(...)`
  as a pseudo-framework
- the public test surface is typed, documented, and stable enough to teach
