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
   backend IR — and the backend IR compiles to machine code. The backend IR
   is a serializable linear SSA form (CLIF / LLVM IR class), not a stack-VM
   bytecode; IR serialization is the incremental cache boundary. The dev
   profile uses a thin pipeline: typed HIR lowers to backend IR with minimal
   passes and per-module parallel codegen; deep SSA passes run only in the
   release profile. The serialized IR cache uses a compact binary format
   (zero-copy class, e.g. rkyv), never textual dumps, so disk I/O cannot
   eat the incremental compile budget. Packaging the bootstrap evaluator
   inside a host executable is a bootstrap workaround, not a native build.
   The end state has no evaluator in release artifacts.
2. **Faster feedback than Rust, Go-like operation.** Incremental compilation
   with a fast dev profile; optimized release profile for throughput. Deploy
   story: static executable, no runtime install, goroutine-simple
   concurrency.
3. **Value Ownership before codegen (Swift / Mojo / Val class).** One chosen
   paradigm, fixed by RFC, with honest tradeoffs — not an implicit "best of
   all worlds". Three taxa: **value types** are `Copy` with copy-on-write
   and a uniqueness fast path; **owned types** (starting with `Box[T]`) are
   move-only with local use-after-move checking; **handle types** are
   shared, reference-like runtime resources — the only user-visible
   aliasing, by design. Plain method receivers borrow for reading; `mut`
   receivers get exclusive in-place access. No global lifetimes and no
   borrow checker — analysis is strictly local. Cross-task data sharing
   goes through channels and shared handle objects; value payloads crossing
   a task boundary transfer when unique and deep-copy deterministically
   when shared. Deterministic resource release (`defer`, final-reference
   close for handles); no tracing GC in the native target; escape analysis
   is a backend optimization, orthogonal to semantics.
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
| 0 — Architecture gates | core-type-system-and-syntax audit (bounded scope, compact delta); memory model RFC; backend ADR; core RFC drafts (protocol, defer, unsafe, closures, generics, Option, tuples) | gate artifacts reviewed in; roadmap synced; delta exact |
| 1 — Semantic core | everything the spec promises works; closures, generics, `Option[T]`, tuples, full patterns, module visibility, const expressions. Complex features are designed as typed-HIR/MIR semantics from day one (generics monomorphize in typed HIR for release, but dev builds dispatch generics through witness tables with instance caching at module boundaries so monomorphization never eats the dev compile budget; async lowers to a MIR state machine; closures use explicit capture lowering); the evaluator is only a conformance oracle, never gold-plated | core frozen; conformance green; every feature spec'd + tested + exemplified; semantics portable to Phase 2 without rewrites |
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
