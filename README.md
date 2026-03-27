# gof

A programming language with Python-like readability, Go-like concurrency, and Rust-grade engineering discipline.

```gof
enum JobState:
    Ready
    Running(pid: int)
    Failed(message: string)

fn score(state: JobState) -> int:
    match state:
        JobState.Ready:
            return 0
        JobState.Running(pid):
            return pid
        JobState.Failed(message):
            return len(message)

fn main() -> int:
    return score(JobState.Running(42))
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
After installation, open a new shell and use `gof` directly. Rust is not required to run `gof` programs.

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
| Functions, typed params, parameterized type annotations, return contracts | stable |
| `struct`, payload `enum`, exhaustive `match` with destructuring | stable |
| `Result[T, E]`, `Result.Ok`, `Result.Err`, postfix `?`, exhaustive `match` over `Result` | stable |
| Methods, field access, constructors | stable |
| `if`/`else`, `while`, `for`, `break`, `continue`, logical ops, explicit equality/ordering, unary `-`, `/`, `%` | stable |
| Lists, dict literals, indexing, dict views | stable |
| `print`, `assert`, `argv`, `env`, `cwd`, file I/O, path/fs helpers, string helpers, sequence helpers (`first`/`last`/`slice`/`reverse`/`sort`/`min`/`max`), conversion helpers, `range`, `sleep(...)` | stable |
| JSON helpers and bootstrap `http_get(...)` / `http_post(...)` | bootstrap |
| Same-directory imports | stable |
| `go`, `await`, typed channels, `close`, cancellation tokens, `select` | bootstrap |
| `gof build --native` | bootstrap (wraps evaluator) |
| Formatter, conformance tests | stable |

See the [language spec](spec/language-v1.md) for the full contract.

## Documentation

| | |
|---|---|
| **[The gof Book](https://gofman5.github.io/gof-lang/)** | Learning path - start here |
| **[The gof Book (RU)](https://gofman5.github.io/gof-lang/ru/)** | Russian edition |
| [Examples](examples/) | Runnable programs |
| [Telegram bot example](examples/telegram_long_polling.gof) | Long-polling baseline |
| [Language spec](spec/language-v1.md) | Formal contract |
| [Diagnostics spec](spec/diagnostics.md) | Error behavior |
| [Roadmap](roadmap.md) | Milestones and current focus |
| [RFCs](rfcs/) | Language evolution proposals |
| [ADRs](adrs/) | Architecture decisions |

## Building and hacking from source

You only need Rust if you are developing the compiler, runtime, or CLI itself.
Normal language usage should go through the installed `gof` binary.

Requires Rust toolchain (see [rust-toolchain.toml](rust-toolchain.toml)) if you
want to work on the repository internals.

```bash
cargo test --workspace                 # run all tests
cargo run -p gof-cli -- run app.gof    # developer fallback without installing gof
```

Preview the books locally:

```bash
mdbook build docs/book
mdbook build docs/book-ru
```

## Project structure

```text
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
