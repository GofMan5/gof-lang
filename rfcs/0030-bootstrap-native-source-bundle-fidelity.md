# RFC 0030: Bootstrap-Native Source Bundle Fidelity

- Status: accepted
- Area: tooling, native build, package/import fidelity

## Summary

Make `gof build --native` preserve the same supported import and local package
behavior as `gof run` by embedding a deterministic source bundle instead of
reconstructing the entrypoint as a synthetic standalone file.

This RFC does not claim a real direct-codegen backend. It only makes the
bootstrap-native path semantically faithful for the import/package surface that
already exists.

## Problem

The previous bootstrap-native runner embedded only the entrypoint text and then
recreated it as:

```rust
SourceFile::new("embedded.gof", ...)
```

That broke two important guarantees:

- same-directory imports depended on the launch working directory of the built
  binary instead of the original source tree
- manifest-backed local package imports were lost because the package context
  was no longer discoverable from the synthetic path

As a result, `gof build --native` worked only for trivial single-file programs
and silently diverged from `gof run`.

## Decision

Introduce a compiler/runtime source-loading abstraction with two backends:

- filesystem-backed source loading for normal CLI compilation and execution
- embedded source-bundle loading for bootstrap-native executables

At native-build time the CLI now embeds a deterministic bundle containing:

- the entry source
- every reachable same-directory import
- every reachable manifest-backed local package source
- per-source package context metadata required for local package resolution

The generated host runner deserializes that bundle and executes through the
same evaluator pipeline against the embedded provider instead of the host
filesystem.

## Consequences

### What improves

- `gof build --native` preserves supported import/package behavior
- built binaries no longer depend on the current working directory for bundled
  imports
- the bootstrap-native path is honest enough to keep shipping while direct
  codegen is still in progress

### What does not change

- the native path is still evaluator-backed
- there is still no final codegen backend, ABI story, or debug-info story
- dynamic discovery outside the bundled reachable graph is still out of scope

## Non-goals

This RFC does not add:

- direct native code generation
- dynamic plugin/module loading from arbitrary host paths
- registry-backed package bundling
- standalone distribution of non-source package artifacts

## Follow-on contract

This integrity slice is only considered complete when the repository also keeps
the surrounding CLI honest:

- package-aware `gof test` executes manifest-backed executable targets
- library package targets remain explicitly compile-only for now
- docs/spec/roadmap describe the bootstrap-native limitations without
  underselling the fidelity improvement
