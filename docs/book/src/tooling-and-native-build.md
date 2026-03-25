# Tooling and Native Build

`gof` already has a unified CLI:

- `gof run`
- `gof build`
- `gof test`
- `gof fmt`
- `gof mod`
- `gof doc`
- `gof bench`

## Build outputs

By default:

```bash
gof build program.gof
```

emits the structured backend artifact.

With:

```bash
gof build program.gof --native
```

the CLI emits a runnable host executable.

## Important honesty rule

Today `--native` is a bootstrap-native path. The generated executable packages the bootstrap evaluator around the source program.

That is useful because:

- users can run a native host executable now
- install and packaging flows can already target a real binary
- the project can harden build ergonomics before direct codegen is finished

But it is not the final backend. The long-term goal is direct native code generation without embedding the bootstrap evaluator.

## Building the book

The learning docs are scaffolded as an `mdBook`.

If you want a local preview:

```bash
cargo install mdbook
mdbook serve docs/book
```
