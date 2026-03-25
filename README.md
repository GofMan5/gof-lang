# gof

`gof` is a new programming language project that aims for:

- Python-like readability and low ceremony
- Go-like concurrency and operational practicality
- Rust-grade engineering discipline and reliability
- a clean path to native performance without semantic chaos

This repository currently contains a bootstrap compiler, formatter, evaluator, test harness, and language specification for the actively supported subset of `gof`.

## Status

`gof` is in active bootstrap development.

What is already real:

- indentation-aware syntax
- top-level functions
- same-directory imports
- typed parameters and return contracts
- immutable bindings by default, `mut` for reassignment
- `if` / `else`
- `while`
- logical operators `and`, `or`, `not`
- lists, indexing, builtin `len(...)`
- task spawning through `go`
- waiting on tasks through `await`
- user-defined `struct` types with typed fields
- struct constructors like `Point(3, 4)`
- field access like `point.x`
- deterministic formatter
- fixture-based conformance tests
- compiler pipeline through `Lexer -> CST -> AST -> HIR -> Typed HIR -> MIR -> SSA -> backend artifact`

What is not finished yet:

- native machine-code backend
- enums and `match`
- channels and `select`
- real standard library
- package registry and full resolver
- production-grade runtime

Important: `gof build` still emits a structured SSA/backend artifact, not a final native executable.

## Design Principles

- `gof` is not a Python compatibility layer
- readability must not destroy optimizability
- safety and predictability beat surface-level convenience
- bootstrap shortcuts are not allowed to become permanent architecture
- new features must land with tests, diagnostics, examples, and spec updates

Project operating rules are documented in:

- [AGENTS.md](./AGENTS.md) for local agent rules
- [roadmap.md](./roadmap.md) for milestones and checkpoints
- [spec/language-v1.md](./spec/language-v1.md) for the current language contract
- [spec/diagnostics.md](./spec/diagnostics.md) for the diagnostics contract

## Example

```gof
struct Point:
    x: int
    y: int

fn score(point: Point) -> int:
    return point.x + point.y

fn main() -> int:
    point: Point = Point(20, 22)
    return score(point)
```

Running this program prints:

```text
42
```

## Quick Start

Run a fixture:

```bash
cargo run -q -p gof-cli --bin gof -- run tests/fixtures/pass/hello.gof
```

Install the latest released `gof` binary on Unix-like systems:

```bash
curl -fsSL https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.sh | bash
```

Install the latest released `gof` binary on Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.ps1 | iex
```

Run the same installer command later to update to the newest release.

Install a specific tagged release on Unix-like systems:

```bash
curl -fsSL https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.sh | bash -s -- v0.1.0
```

The installer places the binary in `$HOME/.gof/bin` by default and updates your shell path.

Run an example:

```bash
cargo run -q -p gof-cli --bin gof -- run examples/geometry.gof
```

Format a file:

```bash
cargo run -q -p gof-cli --bin gof -- fmt path/to/file.gof
```

Check formatting without rewriting:

```bash
cargo run -q -p gof-cli --bin gof -- fmt path/to/file.gof --check
```

Run the fixture suite:

```bash
cargo run -q -p gof-cli --bin gof -- test tests/fixtures
```

Build the current backend artifact:

```bash
cargo run -q -p gof-cli --bin gof -- build examples/geometry.gof
```

Run all workspace tests:

```bash
cargo test --workspace
```

Create or refresh the rolling snapshot prerelease from your local machine:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/publish-snapshot.ps1
```

Push and refresh the snapshot in one step:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/push-and-release.ps1
```

## Current Language Surface

The current bootstrap subset supports:

- `import name`
- `struct`
- `fn`
- typed parameters
- explicit return annotations
- `return`
- `if` / `else`
- `while`
- immutable and mutable bindings
- integer, string, and boolean literals
- list literals
- arithmetic with `+`, `-`, `*`
- logical operators with short-circuit semantics
- comparisons with `==`, `!=`, `<`, `<=`, `>`, `>=`
- top-level function calls
- struct constructors
- field access
- list indexing
- builtin `len(...)`
- `go` and `await`

See [examples/README.md](./examples/README.md) for runnable examples.

## Repository Layout

- `compiler/` - language frontend, typing, IR lowering, and bootstrap evaluator
- `runtime/` - runtime contracts and runtime-facing code
- `stdlib/` - future standard library home
- `tools/` - CLI toolchain
- `tests/` - conformance tests and fixtures
- `benchmarks/` - benchmark harness
- `spec/` - language and diagnostics contracts
- `rfcs/` - language evolution proposals
- `adrs/` - architecture decisions

## Quality Bar

The project targets:

- explicit architectural layering
- strong diagnostics
- 100% coverage target for deterministic compiler/runtime/tooling code
- benchmark-backed performance work
- no public hacks and no fake-complete features

If a feature is not specified, tested, and integrated through the pipeline, it is not considered done.

## Roadmap

The active roadmap is maintained in [roadmap.md](./roadmap.md).

Current priority order:

1. richer data modeling and control flow
2. minimal useful standard library
3. production-grade concurrency model
4. package system and native backend hardening

## CI and Release Automation

The repository now includes:

- CI on every push and on every pull request
- formatting checks
- full workspace tests on Linux and Windows
- benchmark harness smoke builds
- example smoke runs
- release-package smoke builds on Linux, Windows, and macOS for every push and pull request
- release packaging workflows for Linux, Windows, and macOS

Release artifacts are built automatically from version tags and can be consumed by the install scripts in `scripts/install.sh` and `scripts/install.ps1`.

For local maintainer workflows, `scripts/publish-snapshot.ps1` and `scripts/push-and-release.ps1` can also build the current commit and refresh the rolling `snapshot-main` prerelease directly through the GitHub Releases API.
