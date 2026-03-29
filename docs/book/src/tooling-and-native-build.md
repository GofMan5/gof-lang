# Tooling and Native Build

`gof` already has a unified CLI:

- `gof run`
- `gof check`
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

There is now also a script-first watch mode:

```bash
gof run --watch program.gof -- --example arg
```

The current watch contract is intentionally simple and honest:

- initial run starts immediately
- only one run is active at a time
- rapid edits are batched through a small debounce window
- compile/runtime failures do not kill the loop
- the loop keeps using the same execution path as normal `gof run`

For executable manifest-backed packages, watch mode tracks:

- the root package `src/`
- the root `gof.mod`
- the root `gof.lock`
- every locked local dependency `src/`
- every locked local dependency `gof.mod`

If the package graph becomes stale, the loop reports the current lockfile or
manifest problem and waits for the next change instead of silently exiting.

### `gof check`

With:

```bash
gof check program.gof
```

the CLI runs compile-only validation without executing the program.

For editor tooling and automation there is also a machine-readable path:

```bash
gof check program.gof --json --stdin
```

That contract is what the VS Code extension uses for compiler-backed
diagnostics.

### `gof test`

Today package-aware `gof test` has an explicit split contract:

- executable package targets (`src/main.gof`) run compile plus execute smoke
- library package targets (`src/lib.gof`) currently stay compile-only

When the CLI receives an explicit package root, it keeps that package target in
the run even if the package also contains internal `tests/` targets or
fixtures.

That keeps the command honest for runnable packages without pretending the
language already has a dedicated in-language test surface for libraries.

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
bootstrap evaluator around a deterministic embedded source bundle.

That is useful because:

- users can run a native host executable now
- install and packaging flows can already target a real binary
- the project can harden build ergonomics before direct codegen is finished
- reachable same-directory imports and manifest-backed local package imports keep
  working even when the binary is launched from a different current working
  directory

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

What this slice still does not provide:

- REPL
- shebang or direct executable script UX
- script packaging/install flows
- hot reload
- incremental compilation reuse inside watch mode

## VS Code editor baseline

The repository now also ships a VS Code extension for `gof` in
`tools/vscode-gof`, with the primary user-facing install path through the
Marketplace:

- [gof Programming Language](https://marketplace.visualstudio.com/items?itemName=gofman5.gof-language)

It currently covers:

- `.gof` file association
- syntax highlighting
- starter snippets for modules, functions, enums, structs, `select`, and task joins
- compiler-backed diagnostics powered by `gof check --json --stdin`
- `#` comments
- bracket pairs
- indentation for colon-ended blocks
- Marketplace-ready packaging metadata and icon

It does not yet provide:

- LSP
- debugger
- profiler
- semantic rename or go-to-definition

The extension resolves diagnostics toolchains in this order:

1. `gof.toolchain.path`
2. repo-local cargo fallback inside the `gof` repository
3. `gof` on `PATH`

That keeps diagnostics aligned with the actual compiler instead of maintaining a
second, fake validation layer inside the editor. The extension also keeps
diagnostics single-flight per document and cancels stale compiler runs during
rapid edits or config refreshes.

For local packaging:

```bash
cd tools/vscode-gof
npm install
npm test
npm run package
```

That leaves the packaged extension artifact in `tools/vscode-gof/` for manual
Marketplace upload by the maintainer. The repository snapshot release no longer
attaches the editor package by default.

The extension is real editor tooling, but it is still an editor baseline rather
than the full IDE/platform story from M12.

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
