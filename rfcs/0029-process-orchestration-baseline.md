# RFC 0029: Process Orchestration Baseline

- Status: accepted
- Area: stdlib, scripting coverage, operational surface

## Summary

Add `run_process(program, args)` as the first explicit process orchestration
primitive in the bootstrap standard library.

The goal is not to grow a shell DSL. The goal is to let `gof` handle real
automation tasks that currently default to Python or Node, while keeping command
execution explicit, typed, and predictable.

## Motivation

The repository already had:

- environment access through `argv()`, `env(name)`, and `cwd()`
- filesystem access through `read_file`, `write_file`, `read_lines`,
  `write_lines`, `exists`, `read_dir`, `mkdir`, and `remove_file`
- config/data helpers through JSON, CSV, and TOML builtins

That still left a gap in the automation story: users could inspect state and
reshape data, but could not orchestrate external tools from `gof` itself.

Without an explicit process helper, Phase 1 of the competitive roadmap
(`Python`/`Node` wedge for automation and internal tooling) remained incomplete.

## Decision

Add:

```gof
run_process(program, args)
```

with the contract:

- arguments: `(string, list[string])`
- return type: `Result[json, RuntimeError]`
- execution model: direct process spawn, no shell interpolation
- captured report fields:
  - `program: string`
  - `args: list[string]`
  - `status: int`
  - `stdout: string`
  - `stderr: string`

Spawn failures and other host execution failures surface as
`Result.Err(RuntimeError.Io(message))`.

If the host platform cannot provide an integer exit status, the helper also
surfaces that as `RuntimeError.Io(...)` instead of inventing sentinel values.

## Why JSON instead of a bespoke struct

At this stage the stdlib already uses `json` as the explicit bridge type for
configuration, HTTP payloads, and data-wrangling helpers.

Returning `json` here:

- avoids introducing a one-off builtin process-report type too early
- stays consistent with the existing bootstrap operational/data surface
- keeps the report easy to inspect with existing `json_get`, `json_index`,
  `json_string`, `json_int`, and `json_len` helpers

## Non-goals

This RFC does not add:

- shell command parsing
- string-based command concatenation helpers
- pipeline operators
- stdin streaming
- cwd/env override maps
- background process handles
- signal management

Those are future expansions only if the language can keep the semantics
explicit and misuse-resistant.

## Diagnostics

Add:

- `GOF3100`: invalid operand for bootstrap process orchestration helpers

This covers:

- non-string `program`
- non-`list[string]` `args`
- dynamically invalid list contents that survive through `list`-typed wrappers

## Testing and docs

This feature is only complete when it includes:

- type-layer tests
- runtime tests
- pipeline tests
- CLI example coverage
- docs/book updates
- spec and roadmap sync
- VS Code extension syntax/snippet refresh
