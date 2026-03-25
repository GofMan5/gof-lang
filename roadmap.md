# gof Roadmap

## North Star

`gof` is intended to become a language with Python-like readability, Go-like concurrency, and Rust-grade engineering discipline without architectural mud, runtime chaos, or a slow path to production quality.

The real goal is not feature count. Every phase of language development must:

- increase real usefulness
- preserve optimizability
- avoid hacks in syntax, semantics, tooling, or runtime
- keep a clean path toward native code generation and a production-grade toolchain

## Development Rules

- Every major language feature must move through: spec -> parser -> type layer -> execution path -> tests -> examples -> diagnostics -> roadmap sync.
- No surface-level feature is allowed to skip foundational layers.
- Runtime or performance hardening cannot move ahead of semantic stability for the corresponding feature set.
- A milestone is closed only when its exit criteria are met.

## Status Legend

- `[x]` done
- `[~]` in progress
- `[ ]` planned
- `[!]` blocked or intentionally deferred

## Active Focus

- `[~]` M3: richer control flow and data modeling
- Immediate mandatory result: `enum` with explicit variant semantics
- Second mandatory result: `match` with safe branching and an exhaustiveness baseline
- Parallel operational result: release and install ergonomics that make `gof` easy to build, package, install, and update

## Milestone Map

### M0. Governance, Quality, and Distribution Baseline

- `[x]` Repository structure, spec, RFC, ADR, and governance skeleton
- `[x]` Unified CLI with `build`, `run`, `test`, `fmt`, `mod`, `doc`, `bench`
- `[x]` Diagnostics contract and fixture-based conformance harness
- `[x]` Local AGENTS rules for SSS+ quality, tests, and architectural discipline
- `[x]` Public community standards files
- `[x]` CI validation on push and pull request
- `[x]` Release packaging workflow baseline
- `[x]` Cross-platform install scripts for released binaries
- `[x]` Packaging smoke builds on CI for Linux, Windows, and macOS
- `[x]` Local rolling snapshot publish script for every maintainer push
- Exit criteria:
- governance docs exist
- quality rules are explicit
- test, format, bench, package, and install paths are wired and documented

### M1. Frontend Baseline

- `[x]` indentation-aware lexer
- `[x]` parser for functions, bindings, calls, imports, structs, and control flow
- `[x]` deterministic formatter
- `[x]` module graph for same-directory imports
- `[x]` file-aware diagnostics in imported modules
- Exit criteria:
- lexer, parser, formatter, and module loading are deterministic
- positive and negative fixtures cover the supported syntax

### M2. Typed Execution Baseline

- `[x]` typed bindings
- `[x]` explicit return contracts
- `[x]` module-level return inference
- `[x]` `go` / `await` task typing
- `[x]` lists, indexing, builtin `len`
- `[x]` struct declarations, typed fields, constructors, field access
- Exit criteria:
- typed HIR reflects real language contracts
- interpreter semantics match compile-time contracts for the supported subset

### M3. Data Model and Control Flow Expansion

- `[x]` structs as the first user-defined aggregate type
- `[x]` logical operators with short-circuit behavior
- `[ ]` enums with a clear variant model
- `[ ]` branching over states with `match`
- `[ ]` richer equality and comparison rules with explicit semantics
- `[ ]` methods or an equivalent clean receiver story
- Checkpoints:
- `[x]` CP-M3-1: structs with constructor, field access, and imports
- `[x]` CP-M3-2: boolean logic with strict type checking and short-circuit behavior
- `[ ]` CP-M3-3: enums with unit variants, equality, type annotations, and diagnostics
- `[ ]` CP-M3-4: `match` over enum values with an exhaustiveness baseline
- `[ ]` CP-M3-5: receiver or method model without namespace hacks
- Exit criteria:
- user-defined data types can model real domain states
- branching over state is explicit and safe
- no ad hoc runtime magic is required to use core domain modeling

### M4. Minimal Useful Standard Library

- `[ ]` output primitive (`print` or equivalent)
- `[ ]` basic string helpers
- `[ ]` basic list helpers that do not hide allocations
- `[ ]` predictable numeric and conversion utilities
- `[ ]` minimal testing and assert helpers inside the language surface
- Checkpoints:
- `[ ]` CP-M4-1: I/O baseline for CLI apps
- `[ ]` CP-M4-2: zero-surprise helper APIs for strings and lists
- `[ ]` CP-M4-3: stdlib docs and contract tests
- Exit criteria:
- small but real CLI-style programs are possible without compiler-internal hacks

### M5. Concurrency Worth Comparing to Go

- `[x]` task spawning baseline via `go`
- `[x]` typed `await`
- `[ ]` typed channels
- `[ ]` `select`
- `[ ]` cancellation contract
- `[ ]` panic and error propagation across tasks
- `[ ]` scheduler stress and concurrency benchmarks
- Checkpoints:
- `[ ]` CP-M5-1: channels with explicit semantics
- `[ ]` CP-M5-2: `select` semantics and diagnostics
- `[ ]` CP-M5-3: cancellation and propagation rules
- Exit criteria:
- concurrency is useful, typed, testable, and benchmarked
- no hidden global lock or accidental shared mutable-state semantics

### M6. Packages and Modular Growth

- `[ ]` stronger import rules and namespacing
- `[ ]` manifest-resolved package model
- `[ ]` lockfile and deterministic resolution
- `[ ]` registry design without arbitrary code execution
- Checkpoints:
- `[ ]` CP-M6-1: package identity and resolver spec
- `[ ]` CP-M6-2: deterministic dependency graph resolution
- `[ ]` CP-M6-3: local package workflow
- Exit criteria:
- users can grow multi-package programs without ambiguity or hidden scripts

### M7. Native Backend and Runtime Hardening

- `[ ]` transition from SSA JSON artifact to real codegen
- `[ ]` runtime allocation model formalization
- `[ ]` debug info and ABI smoke tests
- `[ ]` performance baselines for startup, memory, throughput, and binary size
- `[ ]` release quality gates based on benchmarks, not marketing
- Checkpoints:
- `[ ]` CP-M7-1: backend IR contract stabilization
- `[ ]` CP-M7-2: first real native artifact path
- `[ ]` CP-M7-3: performance regression gates
- Exit criteria:
- `gof build` emits real executable artifacts
- performance work is benchmark-backed and operationally visible

### M8. Reliability, Tooling, and Production Discipline

- `[ ]` richer diagnostics coverage
- `[ ]` fuzzing for parser, resolver, and type layer
- `[ ]` stress harness for runtime and concurrency
- `[ ]` golden tests for compiler outputs
- `[ ]` observability and debuggability improvements
- Exit criteria:
- production engineering quality is visible in tooling, not just language design

## Immediate Execution Queue

1. `[ ]` Implement enum baseline:
- top-level enum declarations
- variant references
- enum names in type annotations
- imported enums through the module graph
- diagnostics for duplicate and unknown variants

2. `[ ]` Implement `match` baseline:
- syntax
- enum-oriented matching
- exhaustiveness baseline
- diagnostics for missing or duplicate arms

3. `[ ]` Add minimal stdlib output:
- explicit output function
- tests and examples
- no hidden global runtime magic

4. `[ ]` Harden developer distribution flow:
- tagged releases documented in README
- install and update path verified on all supported release targets
- release workflow stays aligned with packaged asset naming

## Current Non-Goals

- macros
- reflection-heavy runtime features
- Python compatibility hacks
- dynamic monkey patching
- package scripts with arbitrary execution
- premature JIT or VM detours
- heavy FFI before language contracts stabilize

## Definition of Done for Any Checkpoint

- code is implemented end-to-end
- tests exist and pass
- diagnostics are updated if behavior changed
- examples exist for the new surface area
- spec is updated
- roadmap status is updated
- no cross-layer hacks were introduced
