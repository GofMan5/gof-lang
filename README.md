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
- builtin list and string helpers through `append(...)` and `contains(...)`
- builtin correctness contracts through `assert(...)`
- builtin file I/O through `read_file(...)` and `write_file(...)`
- builtin dictionary values through `dict()` and `insert(...)`
- task spawning through `go`
- waiting on tasks through `await`
- bootstrap channels through `channel()`, `send(...)`, `recv(...)`, and `select`
- builtin output through `print(...)`
- user-defined `struct` types with typed fields
- user-defined `enum` types with unit variants
- exhaustive `match` over enum values
- explicit struct receiver methods
- struct constructors like `Point(3, 4)`
- enum variant values like `Status.Ready`
- field access like `point.x`
- method calls like `point.total(5)`
- deterministic formatter
- fixture-based conformance tests
- compiler pipeline through `Lexer -> CST -> AST -> HIR -> Typed HIR -> MIR -> SSA -> backend artifact`
- `gof build --native`, which already emits a host executable by packaging the bootstrap evaluator

What is not finished yet:

- direct native code generation without embedding the bootstrap evaluator
- real standard library beyond the current bootstrap helpers
- package registry and full resolver
- production-grade runtime and channel semantics

Important: `gof build` still emits a structured SSA/backend artifact by default. `gof build --native` already produces a real executable, but it currently wraps the bootstrap evaluator instead of using a finalized direct codegen backend.

## Design Principles

- `gof` is not a Python compatibility layer
- readability must not destroy optimizability
- safety and predictability beat surface-level convenience
- bootstrap shortcuts are not allowed to become permanent architecture
- new features must land with tests, diagnostics, examples, and spec updates

Project operating rules are documented in:

- [GOVERNANCE.md](./GOVERNANCE.md) for project governance and engineering policy
- [CONTRIBUTING.md](./CONTRIBUTING.md) for contribution workflow and quality gates
- [roadmap.md](./roadmap.md) for milestones and checkpoints
- [docs/book](./docs/book/) for the versioned learning path and language book
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
The Windows install flow now downloads and runs an `Inno Setup`-built `setup.exe`, which installs `gof` into the current user profile and updates PATH.

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

Build a bootstrap-native host executable:

```bash
cargo run -q -p gof-cli --bin gof -- build examples/hello_print.gof --native
```

Run all workspace tests:

```bash
cargo test --workspace
```

Preview the learning book locally:

```bash
cargo install mdbook
mdbook serve docs/book
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
- `enum`
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
- enum variant references
- exhaustive `match` over enum values
- struct receiver methods via `fn TypeName.method(...)`
- field access
- method calls
- list indexing
- dict indexing with string keys
- builtin `len(...)`
- builtin `print(...)`
- builtin `append(...)`
- builtin `contains(...)`
- builtin `assert(...)`
- builtin `read_file(...)`
- builtin `write_file(...)`
- builtin `dict()`
- builtin `insert(...)`
- builtin `channel()`
- builtin `send(...)`
- builtin `recv(...)`
- `go` and `await`
- `select` over receive arms

See [examples/README.md](./examples/README.md) for runnable examples.

## Learning gof

`gof` should be learned from repository-versioned docs, not from a drifting wiki.

Use this order:

1. [docs/book](./docs/book/) for the teaching path
2. [examples/README.md](./examples/README.md) for runnable examples
3. [spec/language-v1.md](./spec/language-v1.md) for the exact contract
4. [spec/diagnostics.md](./spec/diagnostics.md) for error behavior

The book is intended to grow with the language. Every new public feature should update:

- the book
- examples
- spec
- diagnostics when needed

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

1. richer stdlib beyond the current bootstrap helpers
2. production-grade concurrency semantics beyond the current channel baseline
3. package system and direct native backend hardening
4. runtime and performance hardening

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
Windows release assets now include both `gof-windows-x86_64.zip` and `gof-windows-x86_64-setup.exe`.

For local maintainer workflows, `scripts/publish-snapshot.ps1` and `scripts/push-and-release.ps1` can also build the current commit and refresh the rolling `snapshot-main` prerelease directly through the GitHub Releases API.
