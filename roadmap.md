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

## Detailed Program Plans

`roadmap.md` stays the summary-status index. Ordered implementation programs now
live under `plans/roadmap/`:

- [Roadmap index](plans/roadmap/00-index.md)
- [Language platform program](plans/roadmap/10-language-platform.md)
- [Testing platform overview](plans/roadmap/20-testing-platform/00-overview.md)
- [Testing language surface](plans/roadmap/20-testing-platform/01-language-surface.md)
- [Testing runner, discovery, and CLI](plans/roadmap/20-testing-platform/02-runner-discovery-cli.md)
- [Fixtures, snapshots, and doctests](plans/roadmap/20-testing-platform/03-fixtures-snapshots-doctests.md)
- [Property, fuzz, stress, and bench](plans/roadmap/20-testing-platform/04-property-fuzz-stress-bench.md)
- [CI quality gates](plans/roadmap/20-testing-platform/05-ci-quality-gates.md)

## Status Legend

- `[x]` done
- `[~]` in progress
- `[ ]` planned
- `[!]` blocked or intentionally deferred

## Active Focus

- `[~]` M4: minimal useful standard library for CLI and bot-oriented programs
- `[~]` M5: concurrency semantics beyond task spawn and await
- `[~]` M7: bootstrap-native build path on the road to direct codegen
- `[~]` M8: hybrid language-level test platform slice 2, reporters, and unified product harness follow-through
- `[~]` M10: service/network stdlib delivery, bytes/stream foundations, and reserved import groundwork
- Immediate mandatory result: keep stdlib side effects small, explicit, and allocation-transparent
- Immediate semantic result: keep explicit recoverable error flow moving toward `Result`-based operational APIs
- Second mandatory result: make message-passing concurrency honest and testable
- Product pressure result: make the language capable of a real Telegram long-polling bot before touching web-framework ambitions
- Parallel operational result: keep build, package, install, and update flows aligned with the real toolchain surface

## Competitive Coverage Model

`gof` is not trying to cosplay other languages at the syntax level.
"Covering" Python, Go, Rust, JavaScript, Java, C, and C++ means covering the
real engineering reasons teams choose them, while rejecting their worst legacy
trade-offs.

Coverage targets by source-language strength:

- `Python`: scripting DX, readability, batteries for automation, fast feedback loops
- `Go`: concurrency, deploy simplicity, operational clarity, service-oriented stdlib
- `Rust`: reliability, misuse resistance, explicit contracts, performance predictability
- `JavaScript`: embeddability, event-driven I/O, fast scripting loops, broad integration reach
- `Java`: stability for large codebases, platform tooling, compatibility discipline, observability
- `C`: predictable ABI, direct systems integration, low-level control
- `C++`: zero-cost abstraction discipline, layout control, native-library performance ceilings

Hard interpretation rule:

- `gof` does not need to imitate every feature in these ecosystems
- `gof` does need to cover the strongest practical reasons to pick them
- every parity claim must be backed by semantics, tooling, benchmarks, and production workflows
- no milestone counts as "language coverage" if it only improves syntax without improving engineering capability

## Coverage Program by Capability

- `[~]` Script and automation coverage:
  Python and JavaScript-class CLI, bot, and data-shaping workflows with explicit contracts
- `[~]` Service and concurrency coverage:
  Go-class task orchestration, cancellation, I/O, and deployability
- `[ ]` Native and systems coverage:
  Rust/C/C++-class safety, ABI clarity, predictable cost model, and low-level integration
- `[ ]` Platform and large-codebase coverage:
  Java/C#-class tooling, compatibility discipline, packaging, and observability
- `[ ]` Ecosystem reach coverage:
  package distribution, docs, drivers, templates, web/service baselines, and migration paths

## Three-Phase Competitive Execution Plan

The language should not try to attack every incumbent at once.
The execution path must create one credible wedge at a time, each of which
unlocks the next.

### Phase 1. Python and Node Wedge

- `[~]` Goal: become obviously stronger than Python/Node for correctness-first internal tooling, bots, automation, and data-shaping workflows
- Primary milestone bundle: M4 + M6 + M9
- Why this goes first:
- it compounds fast with the current bootstrap evaluator
- it creates immediate real-user workflows instead of speculative systems claims
- it forces package, stdlib, docs, and DX quality to mature before ecosystem scale
- Mandatory deliverables:
- deterministic package and manifest workflow
- richer automation stdlib for config, time, text, data, filesystem, and process orchestration
- script-first run path with low-friction feedback
- reference automation apps that are cleaner than equivalent Python/Node baselines
- Phase-1 win condition:
- a team can rationally choose `gof` over Python or Node for internal CLI tools, bots, and automation because the code is clearer, safer, and operationally easier to trust

### Phase 2. Go Wedge

- `[ ]` Goal: become a serious alternative to Go for service, worker, and concurrency-heavy operational code
- Primary milestone bundle: M5 + M7 + M10
- Why this goes second:
- concurrency without a stable package/runtime/toolchain base is fake progress
- Go competition is won through deployability, observability, and semantic clarity, not just goroutine-like syntax
- Mandatory deliverables:
- final structured-concurrency contract
- panic and error propagation across task boundaries
- service-grade HTTP/network baseline
- graceful shutdown, deadlines, cancellation, and observability
- real native artifacts with benchmark gates
- Phase-2 win condition:
- `gof` can build and operate real services with a concurrency story that is more explicit and safer than Go without becoming slower or harder to deploy

### Phase 3. Native, Platform, and Ecosystem Closure

- `[ ]` Goal: close the remaining gap against Rust, C, C++, Java, and platform-scale incumbents
- Primary milestone bundle: M11 + M12 + M13 + M14
- Why this goes third:
- systems claims require a real backend, memory model, ABI story, and benchmark discipline
- platform claims require compatibility, tooling, package distribution, and ecosystem depth
- Mandatory deliverables:
- production-grade native backend, ABI, FFI, unsafe boundary model, and performance regression gates
- compatibility policy, workspace tooling, LSP/debugger/profiler, and package registry
- ecosystem reference stacks for CLI, worker, service, library, and embedding workflows
- benchmark-backed competitive scorecards and migration guides
- Phase-3 win condition:
- `gof` is not just pleasant to write; it is measurably competitive across scripting, services, and native systems work with repository-visible proof

Hard sequencing rule:

- do not start wide ecosystem expansion before package identity and compatibility rules exist
- do not claim systems-language parity before native backend, ABI, and benchmark gates are real
- do not claim Go-class service readiness before concurrency, shutdown, and observability are production-grade

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
- `[x]` richer equality and comparison rules with explicit semantics
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
- `[x]` CP-M3-8: explicit structural equality domains and lexicographic string ordering
- Exit criteria:
- user-defined data types can model real domain states
- branching over state is explicit and safe
- no ad hoc runtime magic is required to use core domain modeling

### M4. Minimal Useful Standard Library

- `[x]` output primitive (`print` or equivalent)
- `[x]` basic file I/O baseline through `read_file(...)`, `write_file(...)`, `read_lines(...)`, and `write_lines(...)`
- `[x]` process and filesystem helpers on top of explicit `Result`
- `[x]` path helpers for CLI tooling and bot-oriented programs
- `[x]` basic string helpers
- `[x]` basic list helpers that do not hide allocations
- `[x]` bootstrap dict baseline for key/value data
- `[~]` predictable numeric and conversion utilities
- `[x]` bootstrap JSON helpers for explicit data decoding
- `[x]` bootstrap HTTP GET/POST client plus explicit `sleep(...)` retry helper sufficient for long-polling examples
- `[x]` unary minus, integer division, and modulo in the bootstrap numeric surface
- `[x]` minimal testing and assert helpers inside the language surface
- Checkpoints:
- `[x]` CP-M4-1: I/O baseline for CLI apps
- `[x]` CP-M4-2: zero-surprise helper APIs for strings, lists, dict construction, and dict views
- `[x]` CP-M4-3: process, filesystem, and path helpers on explicit `Result`
- `[x]` CP-M4-4: JSON, HTTP GET/POST, and explicit retry-delay bootstrap path for bot-oriented examples
- `[x]` CP-M4-5: stdlib docs and contract tests
- Exit criteria:
- small but real CLI-style and bot-style programs are possible without compiler-internal hacks

### M5. Concurrency Worth Comparing to Go

- `[x]` task spawning baseline via `go`
- `[x]` typed `await`
- `[x]` typed channels
- `[x]` explicit channel capacity baseline with rendezvous and bounded backpressure
- `[x]` `select`
- `[~]` cancellation contract
- `[x]` channel lifecycle with explicit close semantics
- `[~]` panic and error propagation across tasks
- `[ ]` scheduler stress and concurrency benchmarks
- Checkpoints:
- `[x]` CP-M5-1: channels with explicit semantics
- `[x]` CP-M5-1a: explicit channel capacity baseline for rendezvous and bounded queues
- `[x]` CP-M5-2: `select` semantics and diagnostics
- `[x]` CP-M5-2a: single-`default` fallback baseline for non-blocking `select`
- `[~]` CP-M5-3: cancellation and propagation rules
- `[x]` CP-M5-3a: `close(channel)` plus `Result`-returning `send` and `recv`
- `[x]` CP-M5-3b: token-based cooperative cancellation baseline for channel waits
- `[x]` CP-M5-3c: `await` preserves task-boundary failures inside `Result[..., RuntimeError]`
- `[x]` CP-M5-3d: timeout-backed cancellation token baseline for blocking channel ops
- `[x]` CP-M5-3e: `await_result(task)` provides an explicit recoverable join path for plain `task[T]`
- `[x]` CP-M5-3f: `await_result(task, token)` extends recoverable task joins with cancellation-aware waits
- Exit criteria:
- concurrency is useful, typed, testable, and benchmarked
- no hidden global lock or accidental shared mutable-state semantics

### M6. Packages and Modular Growth

- `[x]` stronger import rules and namespacing
- `[x]` manifest-resolved package model
- `[x]` lockfile and deterministic resolution
- `[ ]` registry design without arbitrary code execution
- Checkpoints:
- `[x]` CP-M6-1: package identity and resolver spec
- `[x]` CP-M6-2: deterministic dependency graph resolution
- `[x]` CP-M6-3: local package workflow
- Exit criteria:
- users can grow multi-package programs without ambiguity or hidden scripts

### M7. Native Backend and Runtime Hardening

- `[x]` bootstrap-native host executable path via `gof build --native`
- `[x]` bootstrap-native embedded source bundles preserve reachable imports and local package graph semantics
- `[ ]` transition from SSA JSON artifact to real codegen
- `[ ]` runtime allocation model formalization
- `[ ]` debug info and ABI smoke tests
- `[ ]` performance baselines for startup, memory, throughput, and binary size
- `[ ]` release quality gates based on benchmarks, not marketing
- Checkpoints:
- `[ ]` CP-M7-1: backend IR contract stabilization
- `[~]` CP-M7-2: first native artifact path
- `[x]` CP-M7-2a: bootstrap-native executables keep same supported import/package behavior outside the source tree
- `[ ]` CP-M7-3: performance regression gates
- Exit criteria:
- `gof build` emits real executable artifacts
- performance work is benchmark-backed and operationally visible

### M8. Reliability, Tooling, and Production Discipline

- `[x]` hybrid language-level testing baseline through `test fn`, shipped `testing` stdlib, snapshots, and `gof test` discovery
- `[x]` opt-in markdown doctest baseline through `gof test --docs`
- `[~]` typed fixtures through `fixture(test|module) fn`, DAG-backed injection, runtime caching, cleanup hooks, artifact-backed product fixtures under `tests/ui|runtime`, plus shipped `gof test --json` / `--junit` reporters and dedicated failure vs harness exit codes
- `[ ]` richer diagnostics coverage
- `[ ]` fuzzing for parser, resolver, and type layer
- `[ ]` stress harness for runtime and concurrency
- `[ ]` golden tests for compiler outputs
- `[ ]` observability and debuggability improvements
- Checkpoints:
- `[x]` CP-M8-0: hybrid testing platform slice 1
- `[~]` CP-M8-1: typed fixtures, cleanup hooks, doctests, product fixtures, shipped JSON/JUnit reporter slices, and dedicated test exit codes
- `[ ]` CP-M8-2: property, fuzz, stress, and benchmark integration
- Exit criteria:
- production engineering quality is visible in tooling, not just language design

### M9. Python and JavaScript-Class Scripting Coverage

- `[~]` script-first workflow with very fast edit-run loops
- `[~]` richer text, collections, time, env, config, and process stdlib
- `[~]` data wrangling baseline for JSON, CSV, templating, and filesystem automation
- `[ ]` REPL or equivalent interactive exploration path
- `[ ]` script packaging and single-command execution ergonomics
- Checkpoints:
- `[~]` CP-M9-1: script runner and module UX that make small automation tasks frictionless
- `[x]` CP-M9-1a: `gof run --watch` serial single-flight run loop for single-file and executable package scripts
- `[~]` CP-M9-2: batteries-included automation stdlib without hidden runtime magic
- `[x]` CP-M9-2a: explicit no-shell process orchestration baseline through `run_process(program, args)` with captured status and output
- `[x]` CP-M9-2b: explicit stdin and line-oriented stdin helpers for shell pipeline automation
- `[x]` CP-M9-2c: explicit Unix wall-clock helpers for automation scripts through `unix_seconds()` and `unix_millis()`
- `[x]` CP-M9-2d: explicit base64 text helpers for HTTP headers, CI payloads, and shell automation
- `[x]` CP-M9-2e: explicit structured HTTP request baseline with headers, timeout policy, TLS-backed transport, and response reports
- `[~]` CP-M9-3: data transformation baseline suitable for real internal tooling
- `[x]` CP-M9-3a: explicit `template_render(template, values)` baseline for dict/json-driven text generation in scripts
- `[x]` CP-M9-3b: explicit `yaml_parse(text)` baseline for automation and CI-style config ingestion through the bootstrap `json` bridge
- Exit criteria:
- most tasks that would rationally be written in Python or Node for local automation can be written cleanly in `gof`
- the language keeps explicit types and predictable costs even in scripting-heavy workflows

### M10. Go-Class Service and Operational Coverage

- `[ ]` structured concurrency finalization
- `[~]` task panic and error propagation across concurrency boundaries
- `[~]` stronger scheduling guarantees and fairness rules
- `[ ]` HTTP server baseline, request routing, and streaming I/O primitives
- `[~]` timeouts, deadlines, context propagation, and service shutdown contracts
- `[ ]` logging, metrics, tracing, and pprof-class observability surface
- Checkpoints:
- `[ ]` CP-M10-1: concurrency semantics strong enough for real service workers
- `[~]` CP-M10-2: network and service stdlib baseline
- `[x]` CP-M10-2a: shipped stdlib module delivery plus reserved import names for `bytes`, `io`, `time`, `net`, and `http`
- `[x]` CP-M10-2b: bootstrap `Bytes`, stream, deadline, socket-address, listener, and TCP-duplex foundations through shipped stdlib modules
- `[ ]` CP-M10-3: operational observability and graceful shutdown baseline
- Exit criteria:
- real services can be built, deployed, debugged, and operated without hiding concurrency or I/O costs
- `gof` has an honest case for workloads that would normally default to Go

### M11. Rust, C, and C++-Class Native Systems Coverage

- `[ ]` real native backend beyond evaluator-wrapped host executables
- `[ ]` formal memory model and aliasing rules
- `[ ]` explicit unsafe boundary design with safe wrappers
- `[ ]` stable C ABI surface and FFI baseline
- `[ ]` predictable layout control for performance-sensitive data types
- `[ ]` zero-copy and low-allocation paths for hot code
- `[ ]` benchmark-backed validation of zero-cost abstraction claims
- Checkpoints:
- `[ ]` CP-M11-1: codegen pipeline capable of serious native artifacts
- `[ ]` CP-M11-2: FFI and ABI contracts suitable for systems integration
- `[ ]` CP-M11-3: performance model, allocator story, and hot-path regression gates
- Exit criteria:
- performance-sensitive libraries and executables can be written in `gof` without hidden runtime ceilings
- safety and FFI contracts are explicit enough to compete with systems-language expectations

### M12. Java-Class Platform, Compatibility, and Large-Codebase Coverage

- `[x]` installable VS Code editor baseline for `.gof` with syntax highlighting, snippets, and compiler-backed diagnostics
- `[x]` rolling snapshot release now ships the installable VS Code extension asset
- `[x]` single-flight VS Code diagnostics cancellation keeps one compiler check per document
- `[ ]` package identity, registry, and deterministic dependency model
- `[ ]` editions or compatibility policy for long-lived codebases
- `[ ]` LSP, debugger, profiler, and richer IDE support
- `[ ]` API docs, symbol indexing, and refactor-aware tooling
- `[ ]` large-workspace build graph and incremental compilation strategy
- `[ ]` diagnostics and observability suitable for multi-team repos
- Checkpoints:
- `[x]` CP-M12-0: installable VS Code editor baseline with syntax-highlighting, snippets, compiler-backed diagnostics, packaging, and snapshot distribution
- `[x]` CP-M12-0a: diagnostics lifecycle cancels stale compiler runs instead of piling up background checks
- `[ ]` CP-M12-1: long-term compatibility and evolution policy
- `[ ]` CP-M12-2: enterprise-scale tooling surface
- `[ ]` CP-M12-3: workspace and package ergonomics for large codebases
- Exit criteria:
- `gof` can support long-lived multi-team codebases without versioning chaos or tooling collapse
- platform stability becomes a feature of the language, not an afterthought

### M13. Ecosystem and Full-Stack Reach Coverage

- `[ ]` package publishing and trusted distribution workflow
- `[ ]` first-party or reference drivers for databases, queues, and web protocols
- `[ ]` web app and API framework baseline with explicit performance contracts
- `[ ]` embeddability story for host applications and plugins
- `[ ]` cross-language integration paths for polyglot systems
- `[ ]` reference templates for CLI, bot, worker, service, and library projects
- Checkpoints:
- `[ ]` CP-M13-1: ecosystem bootstrap with package publishing and discovery
- `[ ]` CP-M13-2: service/web/data reference stacks
- `[ ]` CP-M13-3: embedding and interop baseline
- Exit criteria:
- `gof` can plausibly replace multiple incumbent languages in one organization instead of solving only one niche
- migration cost is reduced by real ecosystem surface, not by marketing claims

### M14. Competitive Closure and Adoption Proof

- `[ ]` benchmark suite against representative Python, Go, Rust, JavaScript, Java, C, and C++ workloads
- `[ ]` migration guides from incumbent languages
- `[ ]` production reference applications that exercise the full language stack
- `[ ]` release criteria tied to performance, stability, and toolchain quality
- `[ ]` public scorecard tracking competitive readiness by capability area
- Checkpoints:
- `[ ]` CP-M14-1: benchmark-backed claims
- `[ ]` CP-M14-2: migration and onboarding paths
- `[ ]` CP-M14-3: reference applications and deployment guides
- Exit criteria:
- claims of "best-in-class" capability are measurable, repeatable, and visible in the repository
- `gof` stops being a promising language experiment and becomes a rational default choice

## Immediate Execution Queue

1. `[~]` Expand the minimal stdlib beyond raw output:
- `[x]` expand string helpers beyond `contains` with `trim`, `split`, `join`, `starts_with`, and `ends_with`
- `[x]` expand list helpers beyond `append` with explicit sequence builders and selectors like `range`, `slice`, `sort`, `min`, and `max`
- `[x]` add dict construction sugar without hiding costs
- `[x]` add deterministic dict view helpers without hiding ordering or allocations
- `[x]` add predictable conversion helpers through `parse_int` and `to_string`
- `[x]` add process, path, and filesystem helpers required by real CLI tools
- `[x]` add explicit no-shell process orchestration with captured stdout/stderr/status
- `[x]` add a bootstrap JSON and HTTP GET/POST client slice plus explicit retry delay for long-polling bots
- `[x]` stdlib docs and contract tests

2. `[~]` Start migrating recoverable operational paths onto explicit results:
- `[x]` language-defined `Result[T, E]` value surface with postfix `?`
- `[x]` migrate process, filesystem, and channel lifecycle APIs onto `Result`
- `[x]` migrate `parse_int` invalid-text handling onto explicit `Result[_, RuntimeError]`
- `[~]` keep runtime diagnostics reserved for invariant failures or temporary bootstrap gaps only

3. `[ ]` Move the bootstrap-native build path closer to direct codegen:
- reduce wrapper overhead in generated host executables
- stabilize artifact naming and smoke coverage for `gof build --native`
- `[x]` preserve supported import and local package semantics through an embedded source bundle
- keep direct codegen milestones honest in docs and tooling

4. `[~]` Deepen concurrency semantics:
- deterministic round-robin select polling baseline plus send/default arms, then stronger fairness and blocking semantics
- finish task propagation over plain `task[T]`, `Result`, and panic boundaries
- concurrency contract tests and benchmarks

5. `[~]` Turn testing into a first-class product surface:
- `[x]` add `test fn`, shipped `testing` stdlib, snapshots, and hybrid `gof test` discovery baseline
- `[~]` split detailed execution plans out of the monolithic roadmap into `plans/roadmap/`
- `[x]` add opt-in markdown doctests through `gof test --docs`
- `[~]` add typed fixtures
- `[~]` add `gof test --json`, `--junit`, and dedicated failure vs harness exit codes; keep property/fuzz/stress and benchmark gates pending

6. `[ ]` Start the competitive coverage closure path:
- package and registry design strong enough for large codebases
- native backend and ABI milestones strong enough for systems work
- service runtime and observability milestones strong enough for Go-class workloads
- script-runner and automation stdlib milestones strong enough for Python/Node-class workflows

7. `[~]` Execute Phase 1 before widening the front:
- `[x]` finish package identity and deterministic local package workflow
- `[x]` make package-aware `gof test` honest for executable package targets while keeping library targets compile-only
- `[x]` make language-level `gof test` discovery, snapshots, and shipped `testing` helpers real enough for repo and user code
- build script-first UX and richer automation/data stdlib
- ship reference apps that make the Python/Node replacement story concrete

## Current Non-Goals

- macros
- reflection-heavy runtime features
- Python compatibility hacks
- mechanical syntax cloning of other languages
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
