# ADR 0003: Runtime Memory Strategy

## Status

Accepted

## Decision

The native target implements the value ownership model from RFC 0043:

- stack-first allocation with backend escape analysis promoting values to
  heap only when they escape or are explicitly boxed
- copy-on-write reference counting for stdlib collections: counters are
  non-atomic while a value is thread-owned; handles (`channel`, `task`,
  `cancel_token`, streams, listeners, the test-runner `TestContext`
  family) use internally synchronized runtime objects instead of CoW
  counters; `Bytes`, `SocketAddr`, and `NetDeadline` are immutable value
  snapshots
- **task-boundary rule:** value payloads crossing a task boundary — `go`
  spawn arguments, channel `send` payloads, and join results — transfer
  their buffer when unique (refcount 1) and receive a deterministic deep
  copy at the boundary when shared (refcount > 1), so a non-atomic counter
  is never mutated from two threads; handle values cross as-is because
  their runtime objects are synchronized
- deterministic destruction at scope exit in reverse creation order, with
  `defer` hooks running first; handles close by **final-reference close**
  (the resource closes when its last live handle is destroyed; dropping a
  task handle detaches the task; dropping a cancel token is a no-op),
  handle `close()` is idempotent, and a close failing during destruction
  surfaces as a located runtime diagnostic while destruction continues
  and reports collected failures once the scope finishes exiting — never
  swallowed and never deferred to a GC
- no tracing garbage collector anywhere in the native target
- `Box[T]` and `Shared[T]` are the explicit heap-indirection surface;
  `Shared[T]` admits only deeply immutable value payloads (typed-HIR
  check: no `mut` fields, no handle fields, recursion only through
  `Shared`), so reference cycles cannot form through it and plain
  reference counts are leak-free

The bootstrap evaluator currently uses **eager deep clones** (`Value::List(Vec<Value>)`
etc., no reference counting) — an implementation detail, not a contract.
Its observable semantics already match RFC 0043 (value duplication, handle
sharing, no user-visible aliasing of data values). The typed-HIR move
checker is the shared static contract, so evaluator and native target
enforce one identical observable memory model. The evaluator is a
conformance oracle, never a performance reference; CoW and the uniqueness
fast path are native-target implementation only.

## Consequences

- no GC pauses exist to measure, but the latency claim still requires
  benchmark proof (Phase 6 scorecard)
- reference-count work is bounded per value lifetime and measured by a
  Phase 2 benchmark gate (refcount ops per value lifetime)
- cross-task data sharing exists only through channels and shared
  handle objects, enforced structurally by the boundary rule rather than
  by runtime locks on data values
- deterministic destruction makes resource leaks testable: scope-exit
  conformance tests assert stream/listener closure ordering
