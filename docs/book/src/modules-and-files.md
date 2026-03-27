# Modules and Files

The bootstrap module system is intentionally small and strict.

That is a feature, not a bug. The project is trying to stabilize semantics before
pretending that a full package ecosystem already exists.

## Local imports

`import name` now resolves through a deterministic local order:

1. `name.gof` next to the importing file
2. `src/name.gof` inside the nearest package root with `gof.mod`
3. `src/lib.gof` from a local path dependency declared as `name = { path = "../dep" }`

Same-directory imports still work exactly as before.

Package-root imports make it possible to keep entrypoints in subdirectories while
sharing one flat package module surface.

Example package:

```text
app/
  gof.mod
  src/
    main.gof
    math.gof
```

with `gof.mod`:

```toml
module = "example/app"
edition = "2026"

[dependencies]
```

and `src/main.gof`:

```gof
import math

fn main() -> int:
    return square(9)
```

## Local path dependencies

The current package workflow also supports local path dependencies.

Example app manifest:

```toml
module = "example/package_app"
edition = "2026"

[dependencies]
package_math = { path = "../package_math" }
```

Dependency layout:

```text
package_math/
  gof.mod
  src/
    lib.gof
    ops.gof
```

`src/lib.gof` is the dependency entrypoint that gets loaded for `import package_math`.

## Locking the local graph

Manifest-backed packages now use a committed `gof.lock`.

Generate or refresh it with:

```text
gof mod resolve --dir package_app
```

Rules:

- `gof.lock` is the only lockfile format
- `gof mod resolve` is the only command that writes it
- `gof run`, `gof build`, and package-aware `gof test` require it for manifest-backed packages
- editing `.gof` source files alone does not stale the lockfile
- changing `gof.mod` metadata or dependency paths does stale the lockfile
- lockfile entries are deterministic and use forward-slash relative paths

## How the current bootstrap graph behaves

Today the module graph:

- works for same-directory imports
- works for package-root imports through the nearest `gof.mod`
- works for local path dependencies through `[dependencies]`
- loads imported files into one merged bootstrap module graph
- rejects import cycles
- rejects duplicate top-level functions
- rejects duplicate top-level structs
- rejects duplicate top-level enums
- rejects conflicting top-level names across functions, structs, and enums

That strictness exists because constructors, type names, and callable names must
stay unambiguous.

## How to organize small programs today

The current best practice is:

- keep a package root with `gof.mod`
- keep executable entrypoints in `src/main.gof`
- keep reusable package exports in `src/lib.gof` for dependencies
- split helper logic into clearly named modules under `src/`
- avoid clever import trees
- keep top-level names distinct and intentional

For example:

- `src/main.gof` for entrypoint logic
- `src/lib.gof` for dependency-facing exports
- `src/math.gof` for math helpers
- `src/models.gof` for `struct` and `enum` declarations

## What is not there yet

Current non-goals:

- no full package registry
- no published dependency resolution flow
- no arbitrary package install scripts
- no large namespacing system yet

That means you should think of the current module system as:

> a deterministic local package workflow with lockfiles, not a finished ecosystem story.
