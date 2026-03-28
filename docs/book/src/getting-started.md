# Getting Started

This chapter is about the shortest reliable path from zero to a running `gof`
program.

## Choose your starting point

You have two good ways to begin:

### Path A: use the released binary

If you want the latest released toolchain:

- Unix-like systems: `curl -fsSL https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.sh | bash`
- Windows PowerShell: `irm https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.ps1 | iex`

This is the right path if you want to try the language as a user.

### Path B: run directly from the repository

If you are developing the language itself or want the freshest bootstrap state:

```bash
cargo run -q -p gof-cli --bin gof -- run examples/hello_print.gof
```

This is the right path if you want to inspect compiler behavior, examples, and tests.

If you only want to write and run `gof` programs, do not stay on the Rust path.
Install `gof`, put it on `PATH`, and use the normal CLI directly.

## Your first program

Write this:

```gof doctest
fn main() -> int:
    print("hello from gof")
    return 42
```

Save it as `hello.gof` and run:

```bash
gof run hello.gof
```

### What happens

- `print("hello from gof")` writes text to standard output
- `return 42` becomes the final displayed program value in the bootstrap CLI path

That split is intentional. `print` is side effect. `return` is program result.

## The first commands you should actually know

Format a file:

```bash
gof fmt hello.gof
```

Check whether formatting is already correct:

```bash
gof fmt hello.gof --check
```

Run the fixture suite:

```bash
gof test tests/fixtures
```

Build the backend artifact:

```bash
gof build hello.gof
```

Build a bootstrap-native executable:

```bash
gof build hello.gof --native
```

If you are hacking on `gof` from the repository before installing it, prefix the
same commands with:

```bash
cargo run -q -p gof-cli --bin gof --
```

## Understand the two build modes correctly

This is one of the easiest places to learn the wrong thing, so be precise:

- `gof run` executes through the bootstrap evaluator
- `gof build` emits the structured backend artifact by default
- `gof build --native` emits a real host executable

But:

> `--native` is still a bootstrap-native path. It does not mean the project already
> has final direct code generation.

That distinction matters because the language is intentionally being honest about its stage.

## A good first study sequence

If you want to learn `gof` efficiently, go in this order:

1. [Language Tour](./language-tour.html)
2. [Types and Data](./types-and-data.html)
3. [Control Flow](./control-flow.html)
4. [Modules and Files](./modules-and-files.html)
5. [Standard Library Baseline](./stdlib-baseline.html)
6. [Concurrency](./concurrency.html)
7. [Tooling and Native Build](./tooling-and-native-build.html)

## What beginners usually misunderstand

> `gof` currently feels small on purpose.

That does not mean it is trivial. It means the project is trying to build a clean
language core before pretending to have a giant ecosystem.

So when something is missing, ask:

- is it not implemented yet?
- is it intentionally delayed?
- or am I trying to use the language like Python instead of like `gof`?

That question will save you a lot of confusion.
