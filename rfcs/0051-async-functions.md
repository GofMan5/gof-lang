# RFC 0051: Async Functions

## Summary

This RFC closes the `async` question for v1 with a defended non-feature
decision plus a pinned design contract:

- **No user-facing async functions in v1.** No `async fn`, no async blocks,
  no futures, no second task type. Concurrency stays the shipped model:
  `go` spawn, `await` / `await_result` join, blocking channel operations,
  `select`, and explicit cancellation tokens.
- **`async` becomes an explicitly reserved keyword.** Every use is a
  compile error with a dedicated diagnostic (`GOF3158`–`GOF3160`) whose
  fix-its rewrite the imported intent onto `go` + `await`. Today the token
  is lexed (`compiler/gof-compiler/src/lexer.rs:373`) and consumed nowhere,
  so `async fn f():` dies in a generic
  ``expected `fn` to start a function`` parse error — a dead promise
  sitting in the v1 surface.
- **The MIR state-machine lowering contract is fixed now, as a dormant
  normative design.** VISION Phase 1 pins the discipline ("async lowers to
  a MIR state machine"); this RFC satisfies it by specifying the lowering —
  states, resume points, frame layout, suspension-point inventory,
  ownership at park/resume, generics restrictions — before any async code
  exists. A future async RFC must adopt this contract verbatim or amend it
  through an ADR.
- **Reopening is a Phase 3 decision with a number.** `async` returns only
  if native-scheduler measurements (per-parked-task memory, park/unpark
  latency, fairness) or a proven structured-concurrency expressiveness gap
  justify it — never as syntax borrowed on faith.

The load-bearing observation: `gof` has no function coloring. Every
function may spawn (`go`), join (`await`), and block (channel operations)
today, so `async fn` would not remove a restriction that exists — it would
add a second spawn notation and a second function class to a language
whose concurrency model is already complete, specified, and shipped.

This RFC is the async design RFC the Phase 1 plan calls for; its
resolution is a non-feature decision, made openly through the RFC process
instead of a silent undershoot of the spec.

## Problem

`async` is the last dead token from the v1 surface, and the decision it
forces blocks Phase 2:

- **The token is unreachable.** The lexer produces `TokenKind::Async` and
  no parser, AST, HIR, or MIR consumer exists for it. Writing
  `async fn f():` today falls into `parse_function`'s
  ``expected `fn` to start a function`` error
  (`compiler/gof-compiler/src/ast.rs`); writing
  `async:` or `async` in expression position falls into generic
  unexpected-token errors. No `.gof` file in the repository uses the token,
  so nothing can break — but nothing can succeed either.
- **The spec promises something the surface cannot parse distinctively.**
  `spec/language-v1.md` lists `async` / `await` in the v1 syntax surface;
  only `await` has semantics (task join). VISION Phase 1's exit criterion
  is "everything the spec promises works", and the repository's gap
  document tracks `async` as one of the remaining Phase 1 design RFCs.
  The promise and the reality disagree; one of them must change.
- **The task runtime ABI and MIR shape are blocked on this decision.**
  ADR 0004 states MIR construction is required "anyway for `async`
  state-machine lowering". Whether task bodies are stackful (a parked task
  owns a stack) or stackless (a parked task is a resumable frame) changes
  backend IR calling conventions, frame layouts, and destruction
  scheduling. Deciding user-facing async syntax now — before the native
  backend and scheduler exist — would fix surface ahead of the measurements
  that could justify it, exactly backwards for this repository.
- **The fork must be resolved, not deferred again.** The options are:
  async functions as spawned state-machine tasks; async as composable
  future values over `go`; or an honest cut. All three are legitimate
  designs; only one is coherent with the shipped go/await model, and the
  reasoning must be on record before Phase 2 backend work starts.

## Proposed Change

### Decision: no user-facing async functions in v1

The v1 concurrency model is unchanged and remains the complete answer:

- `go some_function(...)` spawns a task (top-level named functions today;
  closure targets per RFC 0047 when it ships)
- `await task` joins; `await_result(task)` and `await_result(task, token)`
  are the recoverable and cancellable joins
- blocking `send` / `recv` (with optional cancellation tokens) and
  `select` multiplex; tokens are created, flipped, and scheduled
  explicitly (`cancel_token`, `timeout_token`, `cancel_after`)
- task-boundary ownership, join-result ownership, and the losing-select-
  arm rule are fixed by RFC 0043 and already implemented

Two properties of this model make `async fn` unnecessary in v1:

1. **No coloring.** In Python, JavaScript, Rust, and C#, `async fn` exists
   because a synchronous function cannot suspend — awaiting from sync code
   is impossible or blocks a runtime thread. In `gof`, *every* function can
   already spawn, join, and block: `t = go work(x); value = await t` is
   legal in any body. There is no restriction for `async fn` to lift.
2. **Suspension strategy is unobservable.** Whether a blocked `recv`
   parks a thread (today's bootstrap: one OS thread per task — the
   `std::thread::spawn` in the evaluator's `go` handling) or parks a resumable
   frame (a possible native-backend state machine) is invisible to the
   program: the operation completes when a value arrives, the token wakes
   it when cancelled, the join observes the result. Since the observable
   contract is identical, cheap suspension is a *runtime implementation
   strategy* for the existing `go` semantics — not a new language feature,
   and not a reason to ship new syntax.

Every construct an async-fn user would reach for has a shipped equivalent:

| imported async intent | shipped v1 equivalent |
|---|---|
| `x = await fetch(url)` | `t = go fetch(url)` then `x = await t`; recoverable: `await_result(go fetch(url))` |
| run sub-operations concurrently inside any function | `go` each worker, join with `await` or a channel — legal in every body today |
| timeout on one operation | `await_result(go fetch(url), timeout_token(ms))` |
| first-of-N, cancel the rest | one shared `cancel_token`, workers check it at their blocking ops, `select` over the result channel, `cancel(token)` after the win |
| cancellable wait | `recv(ch, token)` / `send(ch, value, token)` / `await_result(t, token)` |
| large fan-out | `go` per item; the native scheduler owns the M:N mapping (Phase 3) |

### Reserved-token semantics for `async`

`async` remains a lexer keyword (no lexer change, no grammar change, no
extension change — the token was never an identifier). What changes is
that every use fails with a dedicated, actionable diagnostic instead of a
generic parse error:

- `async fn`, `async test fn`, `async fixture(scope) fn`, and
  `async fn Type.method(...)` declarations report `GOF3158`; the parser
  reports it, skips the modifier, and continues parsing the underlying
  declaration so body diagnostics remain meaningful
- an `async:` statement-block prefix (mimicking `unsafe:`) reports
  `GOF3159`
- `async` in any other position — expression, operand, annotation,
  identifier, spawn target — reports `GOF3160`

The reserved status is the honest public contract: `async` is reserved for
a possible future suspension model whose lowering is pinned by this RFC;
v1 concurrency is `go` + `await` + tokens. This replaces two current
behaviors — a dead token in the spec surface and an accidental generic
error at the parse site — with one documented decision.

### Defense against "just use go"

"Just use go" is the recommendation, and it survives the audit:

- **What async would add that gof already has:** cheaper suspension
  (runtime strategy, unobservable), cancellation observation at blocking
  ops (shipped, via explicit tokens), composable first-class work values
  (shipped, `task[T]`), join error propagation (shipped,
  `RuntimeError.TaskFailed` / `TaskPanicked` at joins, `GOF3010`).
- **What async would add that gof must refuse:** a second spawn notation
  (`await f()` next to `go f(); await`), a second function class with its
  own call rules (coloring), implicit token checks at resume points
  (hidden control flow — AGENTS.md §5 rejects it), and a call expression
  that spawns as a side effect (the floating-task hazard class that
  JavaScript lint ecosystems exist to police).
- **What async cannot add at all:** interruption of compute-bound code.
  A state machine suspends only at suspension sites; a hot loop between
  two sites is as uninterruptible as a hot loop between two channel
  operations. Cooperative cancellation is identical in both models:
  observed exactly where the program checks, which in gof is explicit.
- **Sequencing honesty.** The only measurable benefit — memory per parked
  task at high concurrency — cannot be measured until the native scheduler
  exists (Phase 2–3). VISION's honesty rules forbid claims without
  numbers; shipping async syntax now would be a zero-cost claim asserted,
  not benchmarked. The bootstrap evaluator cannot validate a state-machine
  model either: implementing one there is gold-plating a scaffold the
  VISION explicitly demotes to conformance oracle.

### Pinned MIR state-machine lowering contract (dormant, normative)

This section is the design VISION Phase 1 requires to exist from day one.
It is dormant: nothing in it ships in v1, no diagnostics are pre-reserved
for it, and it activates only through a future RFC that passes the revisit
gate below. When activated, the future RFC adopts this contract verbatim
or amends it through an ADR.

**Surface rules (if activated).**

- `async fn name(params) -> Ret:` declares a suspensible function. Its
  body follows named-`fn` rules; return contracts and `expr?` propagation
  behave exactly as in a plain function.
- Calls to async functions are legal only in spawn position:
  `go name(args)` (plus `await go name(args)` as the inline spawn-and-join
  form). A direct call `name(args)` is a compile error. This keeps
  "spawning a task" visible at the spawn site and forbids floating tasks
  created as a call side effect.
- Async function bodies may spawn and join sub-tasks exactly like plain
  bodies: `t = go inner(); x = await t`. Suspension composes through the
  existing task model; there is no special async-to-async call path.
- An async closure, if ever accepted, is a closure capture struct
  (RFC 0047) extended with frame fields; both RFCs' restrictions apply
  unchanged.

**Suspension-point inventory.** Suspension sites are exactly:

- `await` / `await_result` task joins
- blocking `send` / `recv` (including the token forms)
- `select` (one suspension site per select, with multi-source wakeup
  registration for its arms)

Blocking builtins (file I/O, `sleep`, HTTP, process control) and unsafe /
FFI work are not suspension sites: they execute as ordinary blocking
operations on the runtime's blocking workers, with observable semantics
unchanged. How the scheduler isolates blocking work is a Phase 3 runtime
ADR, not a language rule.

**The lowering.** Each async function lowers to one MIR function plus one
frame record type `__async_<module>_<n>`:

- **Frame layout** — a flat record, field order fixed in source order of
  first use, mirroring RFC 0047's capture-struct discipline: a `state`
  field, one slot per parameter, one slot per local whose live range
  crosses a suspension site, one resume-result slot per suspension site,
  and RFC 0045 defer records. Locals dead across every suspension site
  stay stack-local in the resumed segment. Slot taxa follow RFC 0043
  exactly (value slots duplicate under CoW, handle slots share the
  underlying resource, owned slots move).
- **States** — one state per suspension *site* (not per CFG block),
  numbered in source order: deterministic, spannable, diagnostic-friendly.
  Loop bodies re-enter the same state per iteration; the CFG keeps the
  loop, the state selects only the resume point.
- **Suspend protocol** — at a suspension site the site's operand
  expressions are evaluated exactly once, the wakeup sources are
  registered (channel readiness, join completion, token), `state` is
  stored, and control returns `Suspended` to the scheduler. This
  generalizes the shipped `select` contract — arms prepared once at
  select-entry — to whole function bodies.
- **Resume protocol** — the scheduler writes the completed result into the
  site's resume slot and invokes the function with the frame; a jump table
  on `state` lands at the site's resume point, the continuation reads the
  result, and execution proceeds. Invariant: **resume never re-executes
  pre-suspension effects.** Spurious wakeups re-check readiness and
  re-suspend without re-evaluating operands; only the completed operation
  may move an owned payload (RFC 0043's winning-arm rule, preserved).
- **MIR shape** — the existing instruction set (`Spawn`, `Await`,
  `BeginSelect` / `SelectArm` / `EndSelect`, register-based ops) is
  extended with frame-allocation, suspend-with-registration, and
  dispatch-on-state op classes; exact opcode names are fixed by the
  implementation slice's MIR extension under these invariants, which are
  documented as MIR invariants per AGENTS.md §4.

Worked example (future syntax, illustrative only):

```gof
async fn relay(source: channel[int], sink: channel[int]) -> Result[int, RuntimeError]:
    total = 0
    value = recv(source)?
    total = total + value
    send(sink, total)?
    return Result.Ok(total)
```

Suspension sites: S1 at `recv(source)`, S2 at `send(sink, total)`. The
frame holds `state`, `source`, `sink` (parameters, handle taxon), `total`
(live across S2), plus `recv_result` and `send_result` resume slots;
`value` is dead after the addition and gets no slot. Entry runs to S1,
registers on `source`, parks. Resume at S1 writes `recv_result`, the `?`
checks it, `total` updates, S2 registers on `sink`, parks. Resume at S2
checks `send_result`, `Result.Ok(total)` completes the frame, the task
handle stores the value and wakes joiners.

**Ownership at park and resume.** A suspension is *not* a task boundary:
the frame never crosses tasks, so no RFC 0043 boundary rule fires at
park/resume, and no copy or transfer occurs — the frame stays owned by
its task for its whole lifetime. RFC 0043's boundary rules apply where
boundaries actually are: spawn arguments at `go`, `send` payloads at
handoff (an owned payload sits pinned in its frame slot until the
operation completes; the move completes at handoff; cancelled in-flight
sends destroy the pinned payload deterministically at the resumption that
observes the cancellation), and join results at the resume that receives
them (the result crosses a real task boundary and follows the
handle/value/owned rule). Frame destruction at completion runs defer
records first, then reverse-creation-order destruction (RFC 0045, RFC
0043) — the frame is the scope.

**Cancellation, panic, and error contracts (unchanged observable
behavior).** Tokens wake suspended operations with
`Result.Err(RuntimeError.Cancelled)`; tokens never kill tasks; dropping
the last task handle detaches. No implicit token check exists at any
resume point — cancellation stays explicit, per AGENTS.md §5; a future
RFC may add *explicit* token-taking await forms only. Panics unwind the
frame's defers and surface at joins as `RuntimeError.TaskPanicked`
(`GOF3010`), exactly as today. `select` fairness (deterministic
round-robin over ready arms) is preserved by the dispatch discipline.

**Restriction inside generic declarations.** A suspension site may not be
lexically live at a point where the frame's slots would mention the
declaration's type parameters: slot taxon and layout are
instantiation-dependent, and per RFC 0047 / RFC 0048 the erased dev-profile
body and witness tables (method slots only) cannot carry duplication or
destruction operations. Workarounds are the same as closures': hoist to a
concrete type, or route through a channel or handle. The restriction lifts
only through an RFC that extends witness tables with taxon operations —
the same escape hatch RFC 0047 names.

**Evaluator posture.** The bootstrap evaluator executes activated async
functions as ordinary thread-backed tasks with blocking operations —
observably identical, since suspension strategy is unobservable. The
state-machine lowering is native-target implementation verified by
semantics-level conformance, never assumed from the evaluator — the same
posture RFC 0043 takes for CoW.

**Debug contract.** Frames carry the span of their current suspension
site; task failure reports include function, state id, and suspension
site. State ids are source-ordered, so reports are deterministic.

### AGENTS.md §5 answers

For the shipped change (reserved token + diagnostics):

| question | answer |
|---|---|
| who owns the data | no new data; diagnostics only |
| where values live | unchanged |
| where allocation happens | unchanged |
| where blocking can happen | unchanged — channel ops, joins, documented blocking builtins |
| how races are prevented | unchanged — handle types only, RFC 0043 taxa |
| how the feature is debugged | dedicated codes with fix-its replace generic parse errors; exact-code fixtures |

For the pinned contract (design answers the future RFC inherits):

| question | answer |
|---|---|
| who owns the data | the frame is task-owned for its lifetime; slot taxa per RFC 0043 |
| where values live | one flat frame record per task, allocated at spawn |
| where allocation happens | exactly one frame allocation per spawn, at the spawn site |
| where blocking can happen | only at declared suspension sites; other blocking ops run on blocking workers |
| how races are prevented | frames are task-private; the only shared state is handle-typed and internally synchronized |
| how the feature is debugged | source-ordered states, suspension-site spans in task failure reports |

### AGENTS.md §6 checklist for the shipped change

- semantics: none changed — the concurrency contract is the shipped one
- diagnostics contract: `GOF3158`–`GOF3160` with fix-its
- integration tests: exact-code parser fixtures (Test Plan)
- stress plan: not applicable — no runtime change
- benchmark plan: revisit-gate measurements (below)
- cancellation story: unchanged — explicit tokens at explicit operations
- panic/error propagation: unchanged — `TaskFailed` / `TaskPanicked` at joins

### Revisit gate (Phase 3)

`async` reopens only when one of these holds, with numbers on record:

1. **Memory at scale.** Measured resident cost per parked task on the
   native scheduler exceeds the state-machine frame cost by a margin that
   matters on Phase 4-class server workloads at large concurrent-task
   counts (the measurement scale and pass threshold are pinned by the
   Phase 3 scheduler ADR, not asserted here).
2. **Scheduler cost.** Park/unpark latency or a fairness regression
   attributable to stackful task parking, measured on the Phase 3 stress
   gates.
3. **Expressiveness.** Phase 3's structured-concurrency work proves a
   pattern that `go` / `await` / `await_result` / `select` / tokens
   cannot express.

Absent all three, the reserved status stands. The activating RFC must
adopt the pinned lowering contract verbatim or amend it through an ADR,
and allocates its own diagnostics starting at `GOF3163`.

## Alternatives Considered

### Async functions as spawned state-machine tasks (the default direction)

Rejected for v1. The model — `async fn` bodies spawned as tasks, with the
state machine enabling cancellation and select integration — was taken
seriously and is the pinned contract's shape. It fails the v1 filter on
four counts: it adds a second spawn notation and a second function class
to a language with no coloring to fix; its only unique benefit (cheap
suspension) is unobservable and unmeasurable before the native scheduler
exists; it would require gold-plating the bootstrap evaluator with a
second execution model the VISION forbids; and its cancellation story is
either identical to today's explicit tokens (no gain) or implicit
(hidden control flow, rejected). The state machine itself is kept as the
pinned contract so this option remains available with the lowering
already designed.

### Async as future values composing with `go`

Rejected. A `future`-like value distinct from `task[T]` creates two kinds
of "work in flight", a who-runs-it problem for unspawned futures, and a
call expression that spawns as a side effect — the floating-task hazard.
`task[T]` already is gof's first-class unit of in-flight work: passable,
storable, joinable, cancellable. A second unit duplicates it without
adding a capability.

### Evaluator-only async sugar (`async fn` rewritten to `go` + `await`)

Rejected. Sugar that hides a spawn inside a call is the floating-task
hazard again, and fixing surface before the native lowering exists is
backwards: VISION's discipline is semantics from day one, not syntax
from day one. The explicit two-line spawn-and-join keeps intent visible.

### Implicit cancellation at suspension points (ambient task token)

Rejected. Cooperative cancellation in gof is explicit by design; making
resume points check an ambient token hides where blocking can be
interrupted, violating AGENTS.md §5's predictability bar. The pinned
contract forbids it for the same reason.

### Remove the `async` token from the lexer

Rejected. The token was never an identifier, so removal frees `async` as
a binding name — a compatibility hazard the day the token is re-reserved.
Keeping it reserved preserves the activation path with zero grammar
churn, and the extension grammar already keywords it.

### Keep the token dead and silent (status quo)

Rejected. Generic parse errors hide the real contract and leave the spec
promising a surface the parser cannot even identify. AGENTS.md §3 forbids
silent TODOs; the honest form is a documented reservation with dedicated
diagnostics.

### Stackless lowering for all `go` task bodies (no new surface)

Deferred, deliberately. Applying state-machine lowering to every task
body — async as pure implementation — is attractive because it needs no
syntax, but it makes every call inside a task body potentially
suspendable, infecting the ABI of every function with a frame-switching
convention. This is the strategy Zig adopted and later reverted at
scale. The decision belongs to the Phase 3 scheduler ADR with
measurements in hand, decoupled from any syntax decision; the pinned
contract's frame discipline is the design vocabulary that ADR would use.

## Compatibility Impact

- **No valid program changes.** No `.gof` file in the repository uses the
  `async` token; it has never parsed. The change turns accidental generic
  errors into dedicated diagnostics — error code and text change for
  already-invalid programs only.
- **Spec sync (AGENTS.md §8):** `spec/language-v1.md` moves `async` out of
  the "v1 syntax surface" list into a reserved-token note alongside the
  `go` / `await` contract, and rewords the "structured concurrency for
  async work" bullet to "structured concurrency through task spawn and
  join"; `spec/diagnostics.md` gains `GOF3158`–`GOF3160`.
- **Roadmap and plan sync:** `roadmap.md` stops listing async as pending
  Phase 1 semantics and cites this RFC; the repository's agent plan
  documents (which list async among the remaining Phase 1 RFCs) absorb
  the decision — this RFC *is* that design RFC, resolving it as a
  non-feature plus pinned contract.
- **VISION:** the Phase 1 line "async lowers to a MIR state machine" is
  satisfied by the pinned lowering contract — the lowering is decided
  before any async code exists, which is the line's intent; the
  enumerated Phase 1 scope (closures, generics, `Option[T]`, tuples,
  patterns, visibility, const) never listed async. The spec amendment
  above keeps "everything the spec promises works" true by fixing the
  promise instead of undershooting it.
- **Runtime, evaluator, MIR, backend:** unchanged; no pipeline stage
  changes beyond the parser recognizing the reserved modifier to emit the
  new codes.
- **VS Code extension (AGENTS.md §9):** the grammar already highlights
  `async` as a keyword; no grammar or snippet change is required. The
  implementation slice re-runs `npm test` and `npm run package` to confirm
  no drift.
- **docs/site:** `concurrency.mdx` already teaches the go/await model and
  needs no semantic change; `types-and-data.mdx`'s loose "async values"
  phrasing is reworded to task values; `status-and-roadmap.mdx` notes the
  reservation. `npm run build` in `docs/site/` verifies.

## Diagnostics Impact

All three follow the diagnostics contract
(code/severity/span/message/note/fix-it). Numbering: the last shipped
code is `GOF3128` (`spec/diagnostics.md`); RFCs 0043–0050 reserved
`GOF3129`–`GOF3157`; this RFC allocates `GOF3158`–`GOF3160` only and
allocates nothing in `GOF3161`–`GOF3162`. Nothing is pre-reserved for the
dormant lowering contract — the activating RFC allocates from `GOF3163`.

- `GOF3158` (error): reserved `async` modifier on a function-shaped
  declaration — `async fn`, `async test fn`, `async fixture(scope) fn`,
  `async fn Type.method`. Note explains that v1 concurrency is
  `go` + `await` / `await_result`; fix-it deletes the modifier; a second
  note shows the spawn-and-join rewrite for the common
  await-a-function-intent.
- `GOF3159` (error): reserved `async:` block prefix in statement position
  (the `unsafe:` shape). Note explains there are no async blocks; fix-it
  suggests spawning the block's body as a named function with `go` and
  joining with `await`.
- `GOF3160` (error): reserved `async` in any other position — expression,
  operand, type annotation, identifier, or spawn target. Fix-it suggests
  a different identifier name or the `go` form.

Existing codes keep their scope untouched: `GOF3008` (invalid `go` spawn
target), `GOF3009` (`await` / `await_result` requires a task value),
`GOF3010` (task panic surfaced at a join). The parser reports `GOF3158`
and continues parsing the underlying declaration so downstream body
diagnostics remain reachable.

## Test Plan

- parser fixtures (negative, exact code): `async fn f():`, `async test
  fn t():`, `async fixture(test) fn fx():`, `async fn Type.method(...)`,
  each asserting `GOF3158`, the fix-it text, and that the underlying
  declaration still parses far enough to surface its own body errors
  (recovery check)
- statement-position `async:` block asserting `GOF3159` and its fix-it
- expression, annotation, identifier, and spawn-target uses asserting
  `GOF3160`
- positive regression: the existing focused `go` / `await` /
  `await_result` / `select` pipeline and interpreter tests run unchanged
  (they are the coverage that no valid program was affected)
- one fixture pair pinning the rewrite: a program using the reserved
  modifier fails with `GOF3158`; the same program rewritten with
  `go` + `await` passes
- doctests: no new public syntax, so no new doctest surface; the docs
  examples that change wording are rebuilt
- VS Code extension (AGENTS.md §9): `npm test` and `npm run package`
- docs/site: `npm run build` in `docs/site/`

## Benchmark Impact

No runtime behavior changes in v1, so this RFC carries no benchmark gate —
the shipped contract is untouched and the reservation is a compile-time
diagnostic.

The benchmark obligations move to the revisit gate and are recorded here
so they are not lost: the Phase 3 scheduler ADR must measure per-parked-
task resident memory at high concurrent-task counts on Phase 4-class
workloads, park/unpark latency, scheduler fairness under stackful
parking, and the state-machine frame cost as the comparison baseline. The
pinned contract's claims (one frame allocation per spawn, O(1) value-slot
duplication, no copy at park/resume) become enforceable only at that
point; until then they are design invariants, not performance claims.
Both directions honor the honesty rules: no superiority claim for async
without a number, and no zero-cost claim for the cut without the same
number.
