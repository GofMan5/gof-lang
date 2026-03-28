# RFC 0038: Stdlib Import Delivery Baseline

## Summary

`gof` now needs a real shipped standard-library delivery path before adding
typed network and stream APIs.

This RFC establishes the first stdlib import contract:

- shipped stdlib modules live under `stdlib/`
- reserved import names resolve to those shipped modules
- bootstrap-native bundles include reachable stdlib sources the same way they
  already include local modules and manifest-backed packages

## Reserved module names

This first slice reserves:

- `bytes`
- `io`
- `time`
- `net`
- `http`

These names are not ordinary local imports anymore.

If user code declares a same-directory module, package-root module, or local
dependency alias with one of those names, the compiler reports an explicit
diagnostic instead of silently shadowing the shipped stdlib.

## Resolution policy

For reserved stdlib names:

1. resolve the import to `stdlib/<name>.gof`
2. reject local-file or dependency collisions explicitly

For every other import:

1. same-directory `name.gof`
2. nearest package-root `src/name.gof`
3. manifest dependency `src/lib.gof`

This keeps the existing local-package workflow intact while carving out a clean
place for the network and stream surface to grow.

## Native build policy

`gof build --native` must preserve stdlib imports exactly the same way it
already preserves reachable local imports and package context.

That means the embedded source bundle is the source of truth for shipped stdlib
modules during bootstrap-native execution too.

## Non-goals

This slice does not yet define:

- the typed `bytes` / `io` / `net` / `http` API surface
- HTTP server contracts
- TLS policy objects
- metrics or tracing APIs
- a package registry or a full multi-module stdlib edition policy
