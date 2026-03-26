# Tooling and Native Build

`gof` already has a unified CLI:

- `gof run`
- `gof build`
- `gof test`
- `gof fmt`
- `gof mod`
- `gof doc`
- `gof bench`

That unified toolchain matters. One of the goals of the project is to keep the
language easy to operate, not just nice to read.

## What each command means today

### `gof run`

Runs the source program through the bootstrap execution path.

### `gof build`

By default:

```bash
gof build program.gof
```

emits the structured backend artifact.

### `gof build --native`

With:

```bash
gof build program.gof --native
```

the CLI emits a runnable host executable.

## Important honesty rule

Today `--native` is a bootstrap-native path. The generated executable packages the
bootstrap evaluator around the source program.

That is useful because:

- users can run a native host executable now
- install and packaging flows can already target a real binary
- the project can harden build ergonomics before direct codegen is finished

But it is not the final backend. The long-term goal is direct native code generation
without embedding the bootstrap evaluator.

## What good tooling means for `gof`

The project is aiming for tooling that is:

- predictable
- cross-platform
- easy to script
- explicit about what it is producing

That is why this book keeps distinguishing:

- evaluator run
- backend artifact build
- bootstrap-native executable build

Those are different things, and a serious language should teach that difference clearly.

## Building the book

The learning docs are built with `mdBook`.

English and Russian versions are intended to publish together through GitHub Pages.

For a local build of the English book:

```bash
mdbook build docs/book
```

For a local build of the Russian book:

```bash
mdbook build docs/book-ru
```
