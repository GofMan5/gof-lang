# gof

A programming language with Python-like readability, Go-like concurrency, and Rust-grade engineering discipline.

```gof
struct Point:
    x: int
    y: int

fn score(p: Point) -> int:
    return p.x + p.y

fn main() -> int:
    p: Point = Point(20, 22)
    return score(p)
```

> **Bootstrap stage.** The compiler, evaluator, and toolchain are functional and actively developed.
> Native builds work via `gof build --native`. Direct codegen backend is in progress.

## Install

**Unix/macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.sh | bash
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.ps1 | iex
```

Run the same command again to update. The installer places the binary in `~/.gof/bin` and updates PATH.

## Usage

```bash
gof run examples/geometry.gof          # run a program
gof build app.gof --native             # build a native executable
gof fmt src/main.gof                   # format
gof test tests/fixtures                # run conformance suite
```

## What works today

| Area | Status |
|------|--------|
| Functions, typed params, return contracts | stable |
| `struct`, `enum`, exhaustive `match` | stable |
| Methods, field access, constructors | stable |
| `if`/`else`, `while`, `for`-iteration, logical ops | stable |
| Lists, dicts, indexing | stable |
| File I/O, `assert`, `print` | stable |
| Same-directory imports | stable |
| `go`, `await`, channels, `select` | bootstrap |
| `gof build --native` | bootstrap (wraps evaluator) |
| Formatter, conformance tests | stable |

See the [language spec](spec/language-v1.md) for the full contract.

## Documentation

| | |
|---|---|
| **[The gof Book](https://gofman5.github.io/gof-lang/)** | Learning path — start here |
| **[Книга gof (RU)](https://gofman5.github.io/gof-lang/ru/)** | Русская версия |
| [Examples](examples/) | Runnable programs |
| [Language spec](spec/language-v1.md) | Formal contract |
| [Diagnostics spec](spec/diagnostics.md) | Error behavior |
| [Roadmap](roadmap.md) | Milestones and current focus |
| [RFCs](rfcs/) | Language evolution proposals |
| [ADRs](adrs/) | Architecture decisions |

## Building from source

Requires Rust toolchain (see [rust-toolchain.toml](rust-toolchain.toml)).

```bash
cargo test --workspace            # run all tests
cargo run -p gof-cli -- run app.gof   # run via cargo
```

Preview the book locally:

```bash
mdbook build docs/book && mdbook build docs/book-ru
```

## Project structure

```
compiler/    compiler frontend, typing, IR, evaluator
runtime/     runtime contracts
stdlib/      standard library (in progress)
tools/       CLI toolchain (gof-cli)
tests/       conformance fixtures
benchmarks/  benchmark harness
spec/        language and diagnostics contracts
rfcs/        language proposals
adrs/        architecture decisions
docs/book/   English book
docs/book-ru/ Russian book
examples/    runnable .gof programs
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [GOVERNANCE.md](GOVERNANCE.md).

## License

[MIT](LICENSE)
