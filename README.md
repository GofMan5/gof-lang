# gof

`gof` is a greenfield programming language project with Python-like readability, an ahead-of-time native compilation roadmap, and Go-inspired reliability goals.

The current repository is not a marketing mockup. It already contains a working bootstrap compiler pipeline, a CLI, semantic checks, formatter support, conformance fixtures, and benchmark scaffolding.

## Vision

`gof` is designed around a simple rule:

- write code with low ceremony and readable indentation
- compile through a strict, typed pipeline
- keep runtime behavior predictable
- evolve the language through specs, tests, RFCs, and ADRs instead of ad hoc shortcuts

This project borrows Python's feel, not Python's dynamic runtime model.

## Non-goals

`gof` is explicitly not trying to be:

- a CPython-compatible implementation
- a dynamic monkey-patching language
- a metaclass-heavy runtime
- a GIL-based interpreter
- a package ecosystem that runs arbitrary install scripts

## Current Status

The repository currently implements a bootstrap subset of the language.

What works today:

- top-level `fn`
- function parameters
- indentation-sensitive blocks
- `return`
- immutable local bindings through `name = expr`
- mutable local bindings through `mut name = expr`
- reassignment for mutable bindings only
- integer literals
- string literals
- identifiers
- additive expressions with `+` and `-`
- named function calls
- deterministic formatting
- semantic diagnostics for common bootstrap errors

What does not exist yet:

- native machine code output
- imports/modules as working user-facing syntax
- structs, enums, protocols, match, async/await, select, defer, unsafe as implemented features
- a real stdlib surface
- a finished package resolver and registry
- a production runtime with GC, scheduler, and FFI behavior wired into execution

Current `build` output is a structured SSA JSON artifact, not a final executable binary. That is intentional at this stage.

## Example

```gof
fn add(a, b):
    return a + b

fn main():
    base = 40
    mut total = add(base, 1)
    total = total + 1
    return total
```

Running this with the bootstrap CLI prints:

```text
42
```

## Quick Start

Requirements:

- Rust toolchain compatible with `rust-toolchain.toml`

Run the current example:

```bash
cargo run -q -p gof-cli --bin gof -- run tests/fixtures/pass/hello.gof
```

Build a bootstrap SSA artifact:

```bash
cargo run -q -p gof-cli --bin gof -- build tests/fixtures/pass/hello.gof
```

Format a file:

```bash
cargo run -q -p gof-cli --bin gof -- fmt path/to/file.gof
```

Check formatting without rewriting:

```bash
cargo run -q -p gof-cli --bin gof -- fmt path/to/file.gof --check
```

Run language fixtures:

```bash
cargo run -q -p gof-cli --bin gof -- test tests/fixtures
```

Initialize a module manifest:

```bash
cargo run -q -p gof-cli --bin gof -- mod init example/app --dir .
```

Run the full workspace test suite:

```bash
cargo test --workspace
```

Compile benchmark harnesses:

```bash
cargo bench -p gof-bench --no-run
```

## CLI Surface

Current commands:

- `gof build <file.gof>`
- `gof run <file.gof>`
- `gof test [fixtures-dir]`
- `gof fmt <file.gof> [--check]`
- `gof mod init <module> [--edition <edition>] [--dir <path>]`
- `gof doc`
- `gof bench`

## Compiler Architecture

The bootstrap compiler is intentionally layered:

1. indentation-aware lexer
2. CST
3. AST
4. HIR
5. typed HIR
6. MIR
7. SSA
8. backend artifact

Each stage exists to keep responsibilities separated and to prevent "just hack it in" shortcuts from contaminating the rest of the compiler.

## Diagnostics

The compiler already reports deterministic diagnostics with codes and fix-it hints.

Current bootstrap semantic diagnostics include:

- unknown local binding
- reassignment of immutable bindings
- unknown function calls
- wrong function arity
- duplicate local bindings in the same function scope

See:

- `spec/language-v1.md`
- `spec/diagnostics.md`

## Repository Layout

- `spec/`: language, diagnostics, editions, and package-system contracts
- `rfcs/`: public behavior proposals
- `adrs/`: architecture decisions
- `compiler/`: compiler crates
- `runtime/`: runtime contracts and future runtime implementation
- `stdlib/`: standard library roadmap
- `tools/`: user-facing tools such as the `gof` CLI
- `tests/`: conformance fixtures and integration tests
- `benchmarks/`: performance baselines

## Development Rules

This repository is intentionally strict.

Core rules:

- no public language change without spec updates
- no syntax/runtime drift without tests
- no undocumented IR invariants
- no hidden feature flags for user-facing behavior
- no unsafe expansion without isolated ownership and review

If a change affects public language behavior, it should usually touch one or more of:

- `spec/`
- `rfcs/`
- `adrs/`
- `tests/fixtures/`
- compiler tests

## Near-Term Roadmap

The next meaningful steps are:

- imports and module loading
- `if` and loop constructs
- richer type inference and typed bindings
- stronger semantic analysis
- real package resolution
- backend lowering beyond JSON SSA artifacts
- runtime and stdlib bring-up

## Verified Baseline

At the current revision, the bootstrap language and workspace are validated through:

- compiler unit tests
- CLI integration tests
- fixture-based conformance tests
- benchmark harness compilation
- formatter checks

That baseline is enough to keep extending `gof` as a real language project instead of letting the repository collapse into scaffolding and unverified claims.
