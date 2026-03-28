# RFC 0031: Script Watch Run Loop Baseline

## Summary

Add a script-first fast edit-run loop through:

```bash
gof run --watch <target> [-- ...args]
```

This extends the existing `gof run` surface instead of introducing a new
top-level command.

## Decision

The first script UX slice uses `gof run --watch` because:

- it keeps the CLI small and predictable
- it reuses the existing execution contract instead of inventing a parallel path
- it matches the current bootstrap stage better than a larger `script` command family

## Execution model

The watch loop is explicitly serial and single-flight:

- initial run starts immediately
- at most one run is active at a time
- file changes during a run mark the loop dirty
- after the current run finishes, one rerun starts
- rapid edits are batched by debounce

This is not hot reload and not incremental execution reuse.

## Watch roots

For a single-file script:

- watch the entry file directory
- react only to `.gof` changes

For an executable package:

- watch the package root for `gof.mod` and `gof.lock`
- watch the package `src/`
- if a fresh `gof.lock` is present, also watch each locked local dependency
  package root and `src/`

The dependency watch set is refreshed from the current fresh lock graph, not
from ad hoc recursive scanning.

## Failure behavior

Watch mode does not mask failures:

- compile errors are rendered normally
- runtime errors are rendered normally
- stale lockfile or manifest errors are rendered normally
- the loop remains alive and waits for the next change

## Explicit non-goals

This RFC does not add:

- REPL
- shebang support
- script packaging/install UX
- hot reload
- incremental compilation caches inside watch mode
