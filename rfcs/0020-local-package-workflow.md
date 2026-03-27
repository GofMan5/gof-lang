# RFC 0020: Local Package Workflow

## problem

`gof` already has a clean bootstrap module graph, but same-directory imports alone
do not create a credible package workflow:

- package roots are not explicit
- reusable local dependencies cannot be declared in a manifest
- `gof run` and `gof build` do not understand package directories

That leaves M6 blocked on a basic ergonomic step that should exist before any
registry or lockfile work.

## proposed change

Add a deterministic local package workflow around `gof.mod`.

Package layout:

- package root: nearest directory that contains `gof.mod`
- executable entrypoint: `src/main.gof`
- dependency entrypoint: `src/lib.gof`

Manifest shape:

```toml
module = "example/package_app"
edition = "2026"

[dependencies]
package_math = { path = "../package_math" }
```

Import resolution order for `import name`:

1. `name.gof` next to the importing file
2. `src/name.gof` in the nearest package root
3. `src/lib.gof` from a local dependency declared as `name = { path = "../dep" }`

CLI behavior:

- `gof run <package-dir>` resolves `<package-dir>/src/main.gof`
- `gof build <package-dir>` resolves `<package-dir>/src/main.gof`
- `gof mod init` writes `gof.mod` and scaffolds `src/main.gof`

## non-goals

- registry resolution
- semantic version solving
- lockfile generation
- published package installation
- dependency namespacing beyond one flat local alias per manifest

## diagnostics impact

- unresolved package imports still use `GOF3014`
- malformed `gof.mod` files or invalid local dependency paths use `GOF3089`

## test plan

- module-graph tests for package-root imports and local path dependencies
- pipeline coverage for compiling a local package dependency graph
- CLI coverage for running a package directory and for `gof mod init` scaffolding
- example package pair in `examples/`

## rationale

This is the smallest honest M6 slice that makes multi-package local development
real without pretending that registry, semver, or lockfiles already exist.
