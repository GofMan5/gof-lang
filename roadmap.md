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

- `[~]` M4: minimal useful standard library for CLI and bot-oriented programs
- `[~]` M5: concurrency semantics beyond task spawn and await
- `[~]` M7: bootstrap-native build path on the road to direct codegen
- Immediate mandatory result: keep stdlib side effects small, explicit, and allocation-transparent
- Immediate semantic result: keep explicit recoverable error flow moving toward `Result`-based operational APIs
- Second mandatory result: make message-passing concurrency honest and testable
- Product pressure result: make the language capable of a real Telegram long-polling bot before touching web-framework ambitions
- Parallel operational result: keep build, package, install, and update flows aligned with the real toolchain surface

## Milestone Map

### M0. Governance, Quality, and Distribution Baseline

- `[x]` Repository structure, spec, RFC, ADR, and governance skeleton
- `[x]` Unified CLI with `build`, `run`, `test`, `fmt`, `mod`, `doc`, `bench`
- `[x]` Diagnostics contract and fixture-based conformance harness
- `[x]` Runtime-fail fixture coverage for evaluator-backed language contracts
- `[x]` Local AGENTS rules for SSS+ quality, tests, and architectural discipline
- `[x]` Public community standards files
- `[x]` CI validation on push and pull request
- `[x]` Release packaging workflow baseline
- `[x]` Cross-platform install scripts for released binaries
- `[x]` Packaging smoke builds on CI for Linux, Windows, and macOS
- `[x]` Local rolling snapshot publish script for every maintainer push
- `[x]` Windows `Inno Setup` installer with PATH integration for user-level installs
- `[x]` versioned learning book scaffold in `docs/book`
- `[x]` GitHub Pages deployment path for hosted language docs
- Exit criteria:
- governance docs exist
- quality rules are explicit
- test, format, bench, package, install, and docs publish paths are wired and documented

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
- `[x]` parameterized builtin type annotations for list/dict/channel/task/Result
- `[x]` `go` / `await` task typing
- `[x]` lists, indexing, builtin `len`
- `[x]` struct declarations, typed fields, constructors, field access
- Exit criteria:
- typed HIR reflects real language contracts
- interpreter semantics match compile-time contracts for the supported subset

### M3. Data Model and Control Flow Expansion

- `[x]` structs as the first user-defined aggregate type
- `[x]` logical operators with short-circuit behavior
- `[x]` enums with a clear variant model
- `[x]` branching over states with `match`
- `[x]` payload enum variants with typed destructuring in `match`
- `[x]` builtin `Result[T, E]` with postfix propagation and exhaustive result handling
- `[x]` predictable iteration over core iterable values
- `[x]` explicit loop control with `break` and `continue`
- `[ ]` richer equality and comparison rules with explicit semantics
- `[x]` methods or an equivalent clean receiver story
- Checkpoints:
- `[x]` CP-M3-1: structs with constructor, field access, and imports
- `[x]` CP-M3-2: boolean logic with strict type checking and short-circuit behavior
- `[x]` CP-M3-3: enums with unit variants, equality, type annotations, and diagnostics
- `[x]` CP-M3-4: `match` over enum values with an exhaustiveness baseline
- `[x]` CP-M3-4a: payload enum variants and payload destructuring without dynamic runtime tricks
- `[x]` CP-M3-4b: `Result.Ok`, `Result.Err`, exhaustive result `match`, and postfix `?`
- `[x]` CP-M3-5: receiver or method model without namespace hacks
- `[x]` CP-M3-6: `for ... in ...` over lists, strings, and dict keys
- `[x]` CP-M3-7: `break` and `continue` without hidden control-flow hacks
- Exit criteria:
- user-defined data types can model real domain states
- branching over state is explicit and safe
- no ad hoc runtime magic is required to use core domain modeling

### M4. Minimal Useful Standard Library

- `[x]` output primitive (`print` or equivalent)
- `[x]` basic file I/O baseline through `read_file(...)` and `write_file(...)`
- `[x]` process and filesystem helpers on top of explicit `Result`
- `[x]` path helpers for CLI tooling and bot-oriented programs
- `[~]` basic string helpers
- `[~]` basic list helpers that do not hide allocations
- `[x]` bootstrap dict baseline for key/value data
- `[~]` predictable numeric and conversion utilities
- `[x]` bootstrap JSON helpers for explicit data decoding
- `[x]` bootstrap HTTP GET/POST client plus explicit `sleep(...)` retry helper sufficient for long-polling examples
- `[x]` unary minus, integer division, and modulo in the bootstrap numeric surface
- `[x]` minimal testing and assert helpers inside the language surface
- Checkpoints:
- `[x]` CP-M4-1: I/O baseline for CLI apps
- `[~]` CP-M4-2: zero-surprise helper APIs for strings, lists, dict construction, and dict views
- `[x]` CP-M4-3: process, filesystem, and path helpers on explicit `Result`
- `[x]` CP-M4-4: JSON, HTTP GET/POST, and explicit retry-delay bootstrap path for bot-oriented examples
- `[~]` CP-M4-5: stdlib docs and contract tests
- Exit criteria:
- small but real CLI-style and bot-style programs are possible without compiler-internal hacks

### M5. Concurrency Worth Comparing to Go

- `[x]` task spawning baseline via `go`
- `[x]` typed `await`
- `[x]` typed channels
- `[x]` `select`
- `[~]` cancellation contract
- `[x]` channel lifecycle with explicit close semantics
- `[ ]` panic and error propagation across tasks
- `[ ]` scheduler stress and concurrency benchmarks
- Checkpoints:
- `[x]` CP-M5-1: channels with explicit semantics
- `[x]` CP-M5-2: `select` semantics and diagnostics
- `[~]` CP-M5-3: cancellation and propagation rules
- `[x]` CP-M5-3a: `close(channel)` plus `Result`-returning `send` and `recv`
- `[x]` CP-M5-3b: token-based cooperative cancellation baseline for channel waits
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

- `[x]` bootstrap-native host executable path via `gof build --native`
- `[ ]` transition from SSA JSON artifact to real codegen
- `[ ]` runtime allocation model formalization
- `[ ]` debug info and ABI smoke tests
- `[ ]` performance baselines for startup, memory, throughput, and binary size
- `[ ]` release quality gates based on benchmarks, not marketing
- Checkpoints:
- `[ ]` CP-M7-1: backend IR contract stabilization
- `[~]` CP-M7-2: first native artifact path
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

1. `[~]` Expand the minimal stdlib beyond raw output:
- `[x]` expand string helpers beyond `contains` with `trim`, `split`, `join`, `starts_with`, and `ends_with`
- `[~]` expand list helpers beyond `append` with explicit sequence builders like `range`
- `[x]` add dict construction sugar without hiding costs
- `[x]` add deterministic dict view helpers without hiding ordering or allocations
- `[~]` add predictable conversion helpers through `parse_int` and `to_string`
- `[x]` add process, path, and filesystem helpers required by real CLI tools
- `[x]` add a bootstrap JSON and HTTP GET/POST client slice plus explicit retry delay for long-polling bots
- stdlib docs and contract tests

2. `[~]` Start migrating recoverable operational paths onto explicit results:
- `[x]` language-defined `Result[T, E]` value surface with postfix `?`
- `[x]` migrate process, filesystem, and channel lifecycle APIs onto `Result`
- `[~]` keep runtime diagnostics reserved for invariant failures or temporary bootstrap gaps only

3. `[ ]` Move the bootstrap-native build path closer to direct codegen:
- reduce wrapper overhead in generated host executables
- stabilize artifact naming and smoke coverage for `gof build --native`
- keep direct codegen milestones honest in docs and tooling

4. `[ ]` Deepen concurrency semantics:
- select fairness and blocking semantics
- task propagation over `Result` and panic boundaries
- concurrency contract tests and benchmarks

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
