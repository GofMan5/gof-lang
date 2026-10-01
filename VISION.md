# gof Vision

This file is the durable north star for the language completion program. It
defines what "done" means, the hard product constraints that cannot be traded
away, and the phased execution program that gets there without architecture
drift.

`roadmap.md` stays the milestone status index. This vision supersedes it
whenever the roadmap undershoots the target profile below.

## Mission

`gof` combines the strongest practical reasons teams choose Python, Go, Rust,
C, C++, C#, and Java — readability, concurrency, reliability, cost-model
clarity, tooling discipline, platform stability, and scripting ergonomics —
while refusing the inherited defects of each ancestor. The bar is not
feature-count parity. The bar is: `gof` is a rational default for real work
in each ancestor's strongest zone, proven by artifacts.

## Hard constraints (non-negotiable)

1. **Real native compilation, Rust-style pipeline.** Source compiles through
   the layered pipeline — lexer → CST → AST → HIR → typed HIR → MIR → SSA →
   backend IR — and the backend IR (a serializable bytecode) compiles to
   machine code. Packaging the bootstrap evaluator inside a host executable
   is a bootstrap workaround, not a native build. The end state has no
   evaluator in release artifacts.
2. **Faster feedback than Rust, Go-like operation.** Incremental compilation
   with a fast dev profile; optimized release profile for throughput. Deploy
   story: static executable, no runtime install, goroutine-simple
   concurrency.
3. **Explicit memory model before codegen.** One chosen paradigm, fixed by
   RFC, with honest tradeoffs — not an implicit "best of all worlds".
   Working direction: value semantics + move ownership + escape analysis;
   stack by default, heap only where escaping or explicitly requested;
   message passing is the only cross-task sharing; deterministic resource
   release (`defer`); no tracing GC in the native target.
4. **Backend selection by ADR before backend code.** Dev-profile speed and
   release-profile optimization may justify two backends (e.g. Cranelift for
   dev, LLVM for release); the decision lands as an ADR with benchmark
   rationale, not by accident.

## Anti-defect targets

These are verifiable engineering targets, not marketing axioms. Each claim
becomes true only through a benchmark or a test gate:

- no global interpreter lock; parallel tasks map to real parallelism
- no null dereferences — absence is `Option[T]`, failure is `Result[T, E]`
- recoverable errors are values; ignoring one requires explicit intent
- no undefined behavior; memory unsafety only behind explicit `unsafe`
- no implicit type coercion or truthiness chaos
- no hidden global mutable state; sharing is explicit
- small edits keep small compile budgets (incremental gates)
- zero-cost claims are benchmark-backed, not asserted

## Phased completion program

One phase is active at a time. Each phase closes only on its exit criteria.
The agent's living gap document (`.agent-ideal-delta.md`) tracks progress;
`roadmap.md` and `spec/` stay synchronized as the public contract.

| Phase | Scope | Exit criteria |
|---|---|---|
| 0 — Architecture gates | spec-vs-implementation audit; memory model RFC; backend ADR; core RFC drafts (protocol, defer, unsafe, closures, generics, Option, tuples) | gate artifacts reviewed in; roadmap synced; delta exact |
| 1 — Semantic core | everything the spec promises works; closures, generics, `Option[T]`, tuples, full patterns, module visibility, const expressions | core frozen; conformance green; every feature spec'd + tested + exemplified |
| 2 — Native backend | backend IR → first real machine-code artifact → dev/release profiles → incremental compilation → perf gates | `gof build` emits real machine code from IR; compile-budget and throughput benchmarks recorded |
| 3 — Concurrency | structured concurrency, full panic/error propagation, scheduler fairness, stress gates, measured memory/latency model | concurrency contracts tested and stress-gated |
| 4 — Stdlib completeness | TCP/UDP/TLS servers, HTTP server + routing + streaming, collections, crypto basics, property/fuzz/stress/bench testing, observability baseline | service-grade programs possible end-to-end |
| 5 — Platform tooling | package registry (no arbitrary install-time code execution), REPL, LSP, debugger/profiler baseline, extension sync | platform workflows real |
| 6 — Proof | full normative spec, per-feature examples, migration guides, benchmark suite vs Python/Go/Rust, public scorecard | every "better than X" claim has a number |

## Honesty rules

- "Done" means verifiable by code, tests, benchmarks, or a document.
- No claim of superiority without a number from the scorecard.
- No silent TODOs, no feature soup, no layering hacks.
- The bootstrap evaluator is scaffolding, never the product.
