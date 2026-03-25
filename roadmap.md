# gof Roadmap

## North Star

`gof` должен стать языком с Python-like читаемостью, Go-like concurrency и Rust-grade инженерной строгостью, но без архитектурной грязи, непредсказуемого runtime и медленного пути развития.

Главная цель не в количестве фич, а в том, чтобы каждая стадия развития языка:

- увеличивала реальную полезность языка
- сохраняла оптимизируемость
- не вносила костыли в syntax, semantics или runtime
- оставляла чистую траекторию к нативному backend и production-grade toolchain

## Development Rules

- Каждая крупная языковая фича обязана пройти через: spec -> parser -> type layer -> runtime/path execution -> tests -> examples -> diagnostics -> roadmap sync.
- Нельзя перескакивать через фундаментальные слои ради красивой surface feature.
- Нельзя начинать сложный runtime/perf-hardening до стабилизации соответствующей семантики языка.
- Каждая milestone закрывается только при выполнении exit criteria.

## Status Legend

- `[x]` done
- `[~]` in progress
- `[ ]` planned
- `[!]` blocked or deferred pending architecture

## Active Focus

- `[~]` M3: richer control-flow and data modeling baseline
- Ближайший обязательный результат: `enum` + безопасное ветвление по состояниям
- Второй ближайший результат: минимальный stdlib slice для реальной полезности CLI-программ

## Milestone Map

### M0. Governance and Quality Baseline

- `[x]` Repository structure, spec, RFC, ADR, governance skeleton
- `[x]` Unified CLI: `build`, `run`, `test`, `fmt`, `mod`, `doc`, `bench`
- `[x]` Diagnostics contract and fixture-based conformance harness
- `[x]` AGENTS rules for SSS+ quality, tests, architecture discipline
- `[x]` Public community standards files
- Exit criteria:
- governance docs exist
- quality rules are explicit
- test/fmt/bench commands are wired

### M1. Frontend Baseline

- `[x]` indentation-aware lexer
- `[x]` parser for functions, bindings, calls, control flow
- `[x]` deterministic formatter
- `[x]` module graph for same-directory imports
- `[x]` file-aware diagnostics in imported modules
- Exit criteria:
- lexer/parser/formatter/module loading are deterministic
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
- interpreter semantics match compile-time contracts for supported subset

### M3. Data Model and Control Flow Expansion

- `[x]` structs as first user-defined aggregate type
- `[ ]` enums with clear variant model
- `[ ]` branching over states with `match`
- `[ ]` logical operators with short-circuit behavior
- `[ ]` richer comparison/equality rules with explicit semantics
- `[ ]` methods or equivalent clean receiver story
- Checkpoints:
- `[x]` CP-M3-1: structs with constructor + field access + imports
- `[ ]` CP-M3-2: enums with unit variants, equality, type annotations, diagnostics
- `[ ]` CP-M3-3: `match` over enum values with exhaustiveness baseline
- `[ ]` CP-M3-4: receiver/method model without namespace hacks
- Exit criteria:
- user-defined data types can model real domain states
- branching over state is explicit and safe
- no ad hoc runtime magic is required to use core domain modeling

### M4. Minimal Useful Standard Library

- `[ ]` output primitive (`print` or equivalent)
- `[ ]` basic string helpers
- `[ ]` basic list helpers that do not hide allocations
- `[ ]` predictable numeric and conversion utilities
- `[ ]` minimal testing/assert helpers inside language surface
- Checkpoints:
- `[ ]` CP-M4-1: I/O baseline for CLI apps
- `[ ]` CP-M4-2: zero-surprise helper APIs for strings and lists
- `[ ]` CP-M4-3: stdlib docs and contract tests
- Exit criteria:
- small but real CLI-style programs are possible without compiler-internal hacks

### M5. Concurrency That Deserves Comparison to Go

- `[x]` task spawning baseline via `go`
- `[x]` typed `await`
- `[ ]` typed channels
- `[ ]` `select`
- `[ ]` cancellation contract
- `[ ]` panic/error propagation contract across tasks
- `[ ]` scheduler stress and concurrency benchmarks
- Checkpoints:
- `[ ]` CP-M5-1: channels with explicit semantics
- `[ ]` CP-M5-2: select semantics and diagnostics
- `[ ]` CP-M5-3: cancellation + propagation rules
- Exit criteria:
- concurrency model is useful, typed, testable and benchmarked
- no hidden global lock or accidental shared mutable-state semantics

### M6. Packages and Modular Growth

- `[ ]` stronger import rules and namespacing strategy
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

- `[ ]` backend transition from SSA JSON artifact to real codegen
- `[ ]` runtime allocation model formalization
- `[ ]` debug info and ABI smoke tests
- `[ ]` performance baselines for startup, memory, throughput and binary size
- Checkpoints:
- `[ ]` CP-M7-1: backend IR contract stabilization
- `[ ]` CP-M7-2: first real native artifact path
- `[ ]` CP-M7-3: perf regression gates
- Exit criteria:
- `gof build` emits real executable artifacts
- performance work is benchmark-backed, not marketing-backed

### M8. Reliability, Tooling and Production Discipline

- `[ ]` richer diagnostics coverage
- `[ ]` fuzzing for parser/resolver/type layer
- `[ ]` stress harness for runtime and concurrency
- `[ ]` golden tests for compiler outputs
- `[ ]` observability/debuggability improvements
- Exit criteria:
- production engineering quality is visible in tooling, not just language design

## Immediate Execution Queue

1. `[ ]` Implement enum baseline:
- top-level enum declarations
- variant references
- enum names in type annotations
- imported enums through module graph
- diagnostics for duplicate/unknown variants

2. `[ ]` Implement `match` baseline:
- syntax
- enum-oriented matching
- exhaustiveness baseline
- diagnostics for missing or duplicate arms

3. `[ ]` Add minimal stdlib output:
- explicit output function
- tests and examples
- no hidden global runtime magic

## Current Non-Goals

- macros
- reflection-heavy runtime features
- Python compatibility hacks
- dynamic monkey patching
- package scripts with arbitrary execution
- premature JIT/VM detours
- heavy FFI before language contracts stabilize

## Definition of Done For Any Checkpoint

- code is implemented end-to-end
- tests exist and pass
- diagnostics are updated if behavior changed
- examples exist for the new surface area
- spec is updated
- roadmap status is updated
- no cross-layer hacks were introduced
