# Package System

## Files

- source: `*.gof`
- module manifest: `gof.mod`
- lockfile: `gof.lock`

## Manifest shape

```toml
module = "example/app"
edition = "2026"

[dependencies]
mathlib = { path = "../mathlib" }
```

## Lockfile shape

`gof.lock` is written only by:

```text
gof mod resolve [--dir <path>]
```

Example:

```toml
version = 1

[root]
module = "example/app"
edition = "2026"

[[package]]
module = "example/app"
identity = "example/app@<sha256(root-relative-path)>"
manifest_digest = "<sha256(normalized-manifest-metadata)>"
dependencies = ["example/mathlib"]
entry = "src/main.gof"

[package.source]
kind = "path"
path = "."

[[package]]
module = "example/mathlib"
identity = "example/mathlib@<sha256(root-relative-path)>"
manifest_digest = "<sha256(normalized-manifest-metadata)>"
dependencies = []
entry = "src/lib.gof"

[package.source]
kind = "path"
path = "../mathlib"
```

## Local package layout

- package root: nearest directory that contains `gof.mod`
- executable entrypoint: `src/main.gof`
- dependency entrypoint: `src/lib.gof`

## Resolver rules

- `import name` resolves in deterministic local order:
- `name.gof` next to the importing source file
- `src/name.gof` in the nearest package root
- `src/lib.gof` from a local dependency declared as `name = { path = "../some-package" }`
- local dependency paths are resolved relative to the manifest that declares them
- dependency aliases are resolved in sorted alias order from `[dependencies]`
- local dependency paths must point to directories that contain `gof.mod`
- local dependency package entrypoints must live at `src/lib.gof`
- local executable packages use `src/main.gof`
- local package resolution executes no scripts and performs no network access

## Deterministic graph and identity rules

- package identity is `module + sha256(root-relative normalized package path)`
- package source paths in `gof.lock` are serialized with forward slashes
- manifest freshness is based on normalized manifest metadata, not `.gof` source contents
- the same `module` cannot resolve to two different package roots in one local graph
- the same package root cannot appear with conflicting module or edition metadata
- dependency cycles through local manifests are rejected

## CLI contract

- `gof mod resolve [--dir <path>]` is the only command that writes `gof.lock`
- `gof run`, `gof build`, and package-aware `gof test` require a fresh `gof.lock` for any manifest-backed package
- single-file workflows without a surrounding `gof.mod` do not require a lockfile
- `gof run`, `gof build`, and `gof test` never rewrite `gof.lock` implicitly
- stale or missing lockfiles must be repaired by rerunning `gof mod resolve`

## Current limits

- only local path dependencies are implemented today
- no registry resolution yet
- no semantic version solver yet
- no workspace-scale package graph yet
- no arbitrary postinstall scripts
