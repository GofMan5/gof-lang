# RFC 0043: Value Ownership Model

## Summary

Fix the memory and ownership paradigm for `gof` before native codegen work
starts: **Value Ownership** in the Swift / Mojo / Val family, with three
value taxa:

- **value types** — `Copy` with copy-on-write: `int`, `bool`, `unit`,
  `string`, `json`, `list`, `dict`, and enums/structs/tuples/`Result`
  composed of value fields, plus immutable snapshot types `Bytes`,
  `SocketAddr`, `NetDeadline`. Duplication is cheap (O(1) under CoW).
  Reads never consume.
- **owned types** — move-only, starting with `Box[T]`. Using a moved-from
  binding is a compile error.
- **handle types** — shared, reference-like runtime resources: `channel`,
  `task`, `cancel_token`, `ReadStream`, `WriteStream`, `DuplexStream`,
  `TcpListener`, `TestContext`, `TempDir`, `TempFile`. `clone` shares the
  same underlying resource. Aliasing exists only here, by design, and
  handle operations are internally synchronized by the runtime.

Composites (lists, dicts, enums, structs, tuples, `Result`) take their
duplication behavior from their fields: value fields duplicate under CoW;
handle fields share the underlying resource exactly as if cloned directly;
and a composite with any owned field, member, payload, or capture is
itself an owned (move-only) value — ownership is contagious through
composition.

Access rules: analysis is strictly function-local — no global lifetimes, no
borrow checker, no interprocedural aliasing analysis. Cross-task data
sharing happens through channels and shared handle objects. Value payloads
crossing a task boundary transfer if unique or copy deterministically if
shared. Destruction is deterministic: scope exit, reverse creation order,
`defer` first, final-reference close for shared handles. The native target
has no tracing GC.

This RFC is the language-semantics half of the Phase 0 memory gate. Runtime
implementation: ADR 0003. Backend strategy: ADR 0004.

## Problem

`gof` promises Rust-grade reliability without borrow-checker ceremony,
Go-grade operational simplicity, and C-grade cost visibility. Today the
bootstrap evaluator implies a memory model (bindings immutable by default,
`append`/`insert` return new values, channel payloads are handed to
receivers) but nothing fixes the contract:

- Phase 2 needs a decided model **before** backend IR design: stack vs heap
  placement, copy vs move at calls and task boundaries, destruction timing
  must be settled semantics, not backend improvisation
- without a fixed model, "no GC pauses", "no data races", and "zero-cost
  abstractions" are untestable marketing claims
- the classic escape is a borrow checker with global lifetimes (Rust) or a
  tracing GC (Python/Go/Java); both inherit an ancestor defect — ceremony
  tax or hidden runtime cost

## Proposed Change

### Three value taxa

**Value types (Copy + CoW).** `int`, `bool`, `unit`, `string`, `json`,
`list`, `dict`, enums/structs/tuples/`Result` whose fields are all value
types, and the immutable snapshot types `Bytes`, `SocketAddr`,
`NetDeadline`. Binding, argument passing, container insertion, and `match`
payload bindings duplicate them. Copy-on-write makes duplication O(1)
(reference count bump); in-place mutation of a provably-unique value hits
the uniqueness fast path and does not copy. The visible contract of
`clone(value)` is *an independent value*; for CoW types it is O(1) and
materializes lazily on first divergent mutation. Duplication is the
semantic contract — the compiler may elide the copy at a last-use site
(last-use copy elision, observable behavior unchanged), which is what makes
the unique-transfer fast path of the task-boundary rule reachable for
payloads passed from live bindings.

**Owned types (move-only).** `Box[T]` and future explicitly-owned types.
Binding initialization, argument passing, `return`, `go` spawn,
`send(channel, ...)`, and container insertion of owned values **move**
them. Using a moved-from binding is a compile error (`GOF3129`) with a
fix-it suggesting `clone(value)`. `clone` on an owned value produces a deep
independent copy.

**Handle types (shared).** `channel`, `task`, `cancel_token`, `ReadStream`,
`WriteStream`, `DuplexStream`, `TcpListener`, `TestContext`, `TempDir`,
`TempFile`. `clone(handle)` is O(1) and returns a handle to the same
underlying resource. Handles are the only values with user-visible aliasing
— deliberately: they encapsulate synchronized runtime resources (including
the test runner's shared `TestContext` state), their operations are safe
under cross-task sharing, and sharing them across tasks (the shipped
producer/consumer and cancellation patterns) is the intended use. Users
cannot declare new handle-like types.

### Borrow versus move — the full classification

Moves happen **only** where ownership actually transfers, and only for
owned types:

- binding initialization from an owned value
- argument passing of owned values (keep it: `clone(x)` at the call site)
- `return` of owned values
- `go fn(args)` and `send(channel, value)` payloads
- container insertion and struct construction of owned values
- `match` payload bindings of owned payloads (consumes the scrutinee
  binding when a payload moves out)
- `defer` registration of owned arguments (RFC 0045) — the defer record is
  an ownership-transfer site

Everything else **borrows** (reads, never consumes):

- all operators, comparison, equality
- builtin read helpers (`len`, `first`, `last`, `slice`, `reverse`, `sort`,
  `min`, `max`, `contains`, `keys`, `values`, `to_string`, `print`,
  JSON/CSV/TOML/YAML helpers)
- indexing and field access (including handle-typed fields of composites)
- `for binding in iterable:` iteration
- `match` scrutinee evaluation (value-type payloads duplicate, so matching
  never consumes value-type scrutinees)
- `select` arm preparation: arms are prepared once at select-entry and read
  their channel/payload expressions; only the **winning** send arm moves its
  owned payload — losing arms never consume
- method receivers: `fn Type.method(self: Type, ...)` borrows for reading;
  `fn Type.method(mut self: Type, ...)` gets exclusive in-place (`inout`)
  access for the call duration. Because aliasing does not exist for non-
  handle values, exclusivity holds by construction; the receiver parameter
  keeps today's naming rule (any name with the receiver struct type)

Because value types are `Copy`, the shipped read surface (`first`, `slice`,
`sort`, equality, iteration over the same binding) never consumes anything,
and shipped examples like `examples/sequence_helpers.gof` keep compiling.

### Local analysis only

Move checking lives in typed HIR and is function-local binding tracking.
There are no doubtful cases to resolve with clever analysis: value types
duplicate unconditionally; owned types are tracked exactly; handle types
are shared. The compiler never inserts hidden deep copies and never needs
interprocedural analysis. Compile speed is never traded for cleverness.

Loop back-edges are validated as regions: a binding conditionally moved
inside a loop body must be re-initialized on every path that reaches a
later use or the next iteration; otherwise the checker reports `GOF3129`
at the use site.

### Cross-task boundary rule

Task boundaries are: `go` spawn arguments, channel `send` payloads, and
join results (`await` / `await_result` return values). All follow one rule:

- **Handles** cross task boundaries as-is (internally synchronized).
- **Value payloads**: if the evaluated payload is unique (refcount 1), the
  buffer transfers to the receiving task; if it is shared (refcount greater
  than 1), the boundary applies a deterministic deep copy so the receiver
  always gets a thread-owned value. The cost is visible at the boundary,
  never inside later mutation.
- **Owned payloads** move unconditionally — the sender cannot use them
  afterwards, so no sharing can arise.

### Deterministic destruction

- Values are destroyed at scope exit in reverse creation order; `defer`
  statements (RFC pending) run first.
- Handles close their resource by **final-reference close**: the resource
  closes when the last live handle to it is destroyed, so shared handles
  never close a resource another live handle still uses. Dropping a task
  handle detaches the task (it keeps running; joins still go through
  remaining handles). Dropping a cancel token is a no-op.
- `close()` is **idempotent** — explicit `close()` followed by final-
  reference close is safe.
- Fallible close: explicit `close()` methods stay on the typed `Result`
  path for caller-controlled error handling. A close that fails during
  destruction surfaces as a runtime diagnostic with location information —
  never silently swallowed and never deferred to a GC. Destruction
  continues after a failing destructor (remaining values are still
  destroyed in order); the failure is collected and reported once the
  scope finishes exiting.
- The bootstrap evaluator gains final-reference close semantics as part of
  implementing this RFC, so scope-exit conformance tests run against both
  targets.

### Planned stdlib surface

`Box[T]` (unique heap indirection) and `Shared[T]` (deeply immutable,
reference-counted) are named here so the model is complete; their delivery
phase is pinned by the Phase 1 collection-type RFCs, and `roadmap.md`
gains the reference in the Phase 0 roadmap sync.

`Shared[T]` admits only deeply immutable value payloads: no `mut` fields
anywhere in `T`, no handle-typed fields, recursion allowed only through
`Shared`. This is a typed-HIR check, which makes the "immutable `Shared`
cannot form reference cycles, so plain reference counts are leak-free"
claim a checked property rather than an assertion.

### Bootstrap compatibility

The bootstrap evaluator uses eager deep clones internally (`Value::List(Vec<Value>)`
and friends, no reference counting — see ADR 0003) and that is an
implementation detail: the observable semantics of this RFC (value
duplication, handle sharing, no user-visible aliasing of data values)
already hold. The typed-HIR move checker is the shared static contract, so
evaluator and native target enforce one identical model. CoW and the
uniqueness fast path are native-target implementation, verified by
semantics-level conformance, never assumed from the evaluator.

## Alternatives Considered

### Borrow checker with global lifetimes (Rust)

Rejected. Buys zero-cost aliasing control at the price of lifetime ceremony
and compile-time blowup — the ancestor defects `gof` refuses. Value
Ownership gets data-race freedom structurally for data values (no aliasing)
with local-only analysis; handles are synchronized by construction.

### Tracing GC (Python / Go / Java)

Rejected. Hidden pauses, hidden allocation costs, a runtime that can never
be left out of release artifacts. Conflicts with the static-binary and
cost-visibility targets.

### ARC on every value (Objective-C / early Swift)

Rejected. Atomic reference counts on every assignment are invisible cost
and slow multithreaded hot paths. Reference counting appears only where it
is cheap and visible: CoW collections (non-atomic while thread-owned) and
handles/`Shared` (internally synchronized).

### Full keyword surface: `borrow` / `inout` / `sink` / `owned`

Rejected. Four parameter conventions are a ceremony tax. `gof` gets the
same guarantees with existing surface: default duplication for value
types, `mut` receivers, `clone(...)` at the seams, and move rules only for
explicitly owned types.

### Keep the model implicit (status quo)

Rejected. Phase 2 cannot design backend IR, calling conventions, or
destruction scheduling against an undecided memory model.

## Compatibility Impact

- Existing programs keep compiling: value types are `Copy`, the whole read
  surface borrows, and handle sharing across tasks is already the shipped
  pattern (`examples/channel_capacity.gof`, producer/consumer examples).
- New surface: `clone(...)` builtin; `mut` method receivers (new parameter
  position for `mut`); use-after-move checking applies to owned types only
  — no shipped example declares owned types yet, so nothing can break.
- `Box[T]`/`Shared[T]` arrive with later RFCs and do not affect this RFC's
  acceptance.

## Diagnostics Impact

All three follow the diagnostics contract (code/severity/span/message/
note/fix-it):

- `GOF3129` (error): use of a consumed (moved-from) binding; note explains
  the move site; fix-it suggests `clone(value)` at the consuming site
  (owned types are the only types that can move)
- `GOF3130` (error): invalid operand for builtin `clone`
- `GOF3131` (error): `mut` receiver method called on an immutable binding

`spec/diagnostics.md` gains all three codes when the typed-HIR pass lands.

## Test Plan

- typed-HIR unit tests: move tracking for owned types across every
  statement form in the classification above (binding, call, `return`,
  `go`, `send`, container insertion, struct construction, `match` payload)
- loop back-edge cases: conditional moves inside `while`/`for` with and
  without re-initialization on all paths
- positive/negative fixtures: programs that must compile and programs that
  must fail with `GOF3129`/`GOF3130`/`GOF3131`
- boundary stress (AGENTS.md §6): payload ownership on cancelled `send`,
  on losing `select` arms, and on `await_result` cancellation; value
  payload copy-at-boundary when shared; join-result ownership
- destruction tests: final-reference close ordering with shared handles,
  double-close idempotence, destruction continuation after a failing
  destructor close, scope-exit ordering
- conformance equivalence: identical observable behavior under the
  bootstrap evaluator before/after the static checks land
- examples: an ownership walkthrough example (value duplication, handle
  sharing, `clone`, `mut` receiver)
- VS Code extension (AGENTS.md §9): grammar/snippets updated for `mut`
  receivers and `clone`; `npm test` and `npm run package` verified

## Benchmark Impact

Phase 2 gates, not bootstrap claims:

- `clone` on CoW collections is O(1) (reference count bump)
- uniqueness fast path: in-place mutation of a provably-unique collection
  does not copy
- boundary deep-copy rate for shared payloads crossing tasks
- last-use copy elision rate (how often the transfer fast path is reached)
- refcount operations per value lifetime (bounded, measured)
- escape analysis promotion rate: share of allocations that stay on the
  stack
- no zero-cost claim for this model is accepted without a benchmark
