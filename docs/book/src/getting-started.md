# Getting Started

## Install or run locally

If you want the latest released binary:

- Unix-like systems: `curl -fsSL https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.sh | bash`
- Windows PowerShell: `irm https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.ps1 | iex`

If you are developing inside the repository, you can run the CLI directly:

```bash
cargo run -q -p gof-cli --bin gof -- run examples/hello_print.gof
```

## Your first program

```gof
fn main() -> int:
    print("hello from gof")
    return 42
```

Run it:

```bash
cargo run -q -p gof-cli --bin gof -- run hello.gof
```

## Useful first commands

Format a file:

```bash
cargo run -q -p gof-cli --bin gof -- fmt hello.gof
```

Run fixtures:

```bash
cargo run -q -p gof-cli --bin gof -- test tests/fixtures
```

Build the bootstrap backend artifact:

```bash
cargo run -q -p gof-cli --bin gof -- build hello.gof
```

Build a bootstrap-native executable:

```bash
cargo run -q -p gof-cli --bin gof -- build hello.gof --native
```

## How to learn the language in the right order

Do not jump straight into every feature at once.

Recommended path:

1. functions, bindings, and return values
2. types, lists, dicts, and structs
3. control flow with `if`, `while`, and `match`
4. modules and examples
5. channels, `select`, `go`, and `await`
6. tooling, testing, and native build flow
