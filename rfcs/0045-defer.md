# RFC 0045: Defer Statements

## Summary

Implement the `defer` statement — already a lexer token and a listed v1 surface
construct — as explicit scope-exit cleanup:

- `defer <call-expression>` registers a deferred call **at the execution
  point**; the call target and argument expressions are evaluated **at
  registration** (Go semantics), not at execution
- registered calls run **LIFO** when the block they were registered in exits,
  and they run on **every exit path**: normal block completion, `return`,
  `?` early propagation, runtime-diagnostic/panic unwinds, and
  `break` / `continue`
- defer runs **before** the scope's owned-value destruction, completing the
  ordering RFC 0043 fixed: *defers first (LIFO), then destruction in reverse
  creation order*
- defer scopes to the **block it appears in** — a loop-body defer is
  per-iteration and flushes at the end of each iteration (defended below
  against Go's function-scoped alternative)
- the deferred call's result is discarded; a deferred call that completes
  with `Result.Err` or raises a runtime diagnostic surfaces as a **located
  runtime diagnostic (`GOF3138`)** — never silently swallowed — while the
  remaining deferred calls and destruction still complete first
- no `recover`: defer cannot cancel or alter an exit; panics stay invariant
  failures and recoverable errors stay typed `Result` values
- new diagnostics `GOF3136`–`GOF3138`; no other public behavior changes

## Problem

`defer` is listed in the spec v1 surface and tokenized by the lexer
(`TokenKind::Defer`), but the parser has no branch for it, so every use today
is a parse error. Three real gaps depend on that hole:

- **Cleanup-on-error is inexpressible.** The shipped idiom cleans up only on
  the success path. `examples/tcp_roundtrip.gof` calls `conn.close()?` after
  the reads that can fail — when `read_exact_string` propagates an error, the
  connection leaks. RFC 0043's final-reference close eventually reclaims the
  handle, but the *user-visible* close ordering, logging, and non-resource
  cleanup (unlock, send, counter reset) have no syntax at all.
- **RFC 0043 left a pinned dependency.** It fixes destruction as "scope exit,
  reverse creation order, `defer` first" and ADR 0003 repeats it — but the
  `defer` half is undefined until this RFC lands. Phase 2 cannot design
  scope-exit regions in backend IR against an unspecified construct.
- **The Phase 0 gate requires it.** VISION.md lists `defer` among the core
  RFC drafts that close the architecture gates; the spec surface advertises
  it, so the repository currently teaches a construct the compiler rejects.

## Proposed Change

### Statement form and registration

A new statement form, valid anywhere a statement is valid inside a callable
body (`fn`, `test fn`, `fixture(scope) fn`) and in every nested block within
one (`if`/`else`, `while`/`for` bodies, `match` arms, `select` arms):

```gof
defer close(stream)
defer conn.write_all_string(payload)
defer send(channel, value, token)
```

Grammar:

```
defer_stmt := "defer" call_expr newline
```

Executing a `defer` statement:

1. resolves the call target (top-level function, method receiver, builtin)
   and evaluates every argument expression immediately, exactly as an
   ordinary call at that point — `?` inside an *argument* expression
   propagates right then, and the defer is never registered
2. stores the resolved target plus the evaluated argument values in the
   enclosing block activation's pending-defer list
3. continues with the next statement; nothing else observable happens

The deferred pair is subject to the RFC 0043 argument taxa at registration:
value-type arguments duplicate (CoW), handle arguments share the underlying
synchronized resource, and owned-type arguments **move** into the defer
record at registration (later use of the moved-from binding is `GOF3129`,
the ordinary move diagnostic).

The call itself — target resolution, argument evaluation, move checking — is
typed and checked like any other call; only the *scheduling* is new. There is
no function value, no closure, and no new expression form.

### Scope-exit execution: all exits, LIFO

A block scope exits by exactly one path: normal completion (falling off the
end), `return`, `?` propagation, a runtime-diagnostic/panic unwind, or
`break`/`continue` leaving a loop body. Every path runs the same scope-exit
sequence for the block being left:

1. run the block's pending deferred calls in reverse registration order
   (LIFO); a failing deferred call records its failure and execution
   continues with the next (RFC 0043's destruction-continuation rule applied
   to defer)
2. destroy the block's values in reverse creation order (RFC 0043: owned
   destruction, final-reference close for handles)
3. propagate the exit — normal completion continues outward; `return`
   delivers the already-evaluated return value; `?` delivers the propagated
   `Result.Err`; a runtime diagnostic continues to unwind; `break` /
   `continue` performs the loop jump

Nested scopes unwind **innermost first**, each running its own sequence. The
function's return value or propagated error is evaluated before unwinding
starts and is moved out per RFC 0043 — it is not owned by the function's
scopes and is not destroyed by the exit sequence. Concretely:

```gof
fn serve(listener: TcpListener) -> Result[int, RuntimeError]:
    conn = listener.accept_with_timeout_budget(5000)?
    defer conn.close()
    payload = conn.read_exact_string(5)?
    conn.write_all_string(payload)?
    return Result.Ok(len(payload))
```

If either read or write propagates an error, `conn.close()` runs before the
error reaches the caller. The same holds for a runtime diagnostic raised
inside `serve`, and for a task function spawned with `go`: its defers run at
its exit, and only afterwards does the runtime convert a task panic into
`RuntimeError.TaskPanicked(...)`.

LIFO is per-scope, and inner scopes unwind before outer ones — so defers in
different scopes are not globally LIFO:

```gof
fn scoped(flag: bool) -> unit:
    if flag:
        defer print("arm")          # runs at the if-block's exit
    defer print("function")         # runs at the function body's exit
```

This prints `arm` (if taken) then `function` — registration order across
scopes, LIFO within each. That is the honest consequence of block scoping,
not an exception to it.

### Ordering against RFC 0043 destruction

Within one block's exit, defers run before that block's destruction. This
implements RFC 0043's "defer statements (RFC pending) run first" and keeps
the invariant local: a deferred call may still read values owned by the
scope it is leaving (a handle it is about to close, a message it is about to
send), so it must observe them before destruction consumes them. The
defer-before-destruction conformance tests land together with RFC 0043's
scope-exit destruction implementation, since the two share the scope-exit
machinery; until destruction lands in the evaluator, defer's own ordering
contract (all exits, LIFO, before propagation continues) is independently
testable.

### Loop bodies: per-iteration block scope (the decision)

`defer` inside a loop body scopes to the **loop-body block** and flushes at
the end of every iteration — LIFO within the iteration. `break` and
`continue` flush that iteration's defers before jumping. Decision and
defense:

- **Block semantics are the language's uniform rule.** Every indented block
  is a scope; bindings and destruction (RFC 0043) are per-block, and the
  spec itself calls the `for` body an *iteration scope*. Defer attaching to
  the same scope unit yields one rule — "a block exits: defers LIFO, then
  destruction" — with zero special cases for loops.
- **Ownership coherence.** Values created in an iteration are destroyed at
  that iteration's end. A function-scoped defer registered in iteration N
  would run after its arguments' owning scope already exited — a defer
  referencing destroyed values is incoherent with the RFC 0043 model.
- **Go's loop defer is a documented footgun**, not a strength: `defer` in a
  loop holds every resource open until function exit, forcing a wrapper
  function per iteration to get per-iteration release, and grows the defer
  stack with the iteration count. AGENTS.md rejects ancestor baggage.
- **Resource pairing.** Per-iteration LIFO gives acquire → defer-release
  inside one iteration, and the release runs *before* the next acquire:
  bounded resource usage, deterministic pairing, cleanup never skipped on
  early loop exit.

```gof
fn sweep(paths: list[string]) -> Result[int, RuntimeError]:
    total = 0
    for path in paths:
        stream = open_read_stream(path)?
        defer stream.close()
        text = stream.read_all_string()?
        total = total + len(text)
    return Result.Ok(total)
```

Every iteration closes its own stream; a `?` exit from `read_all_string`
still closes the current stream before the error leaves the function. The
one pattern block scoping cannot express is "accumulate in a loop, clean up
once at function exit" — the answer is to register the defer in the function
body (hoisting the acquisition out of the loop), and if real demand appears
a scope modifier is future-RFC territory, not a silent overload of this one.

### Argument evaluation at registration (the decision)

The deferred call's target and arguments are evaluated **when the `defer`
statement executes**, not when the deferred call runs. Defense:

- **Snapshot correctness under `mut`.** `defer close(stream)` must close the
  resource acquired at that line. If arguments were read at exit, a later
  `mut stream = other` would silently redirect cleanup onto the wrong
  resource. Registration-time evaluation binds the action to the acquisition
  point.
- **Exit-time state is unwinding state.** Arguments re-evaluated at scope
  exit would run during an error unwind — where the program state is already
  compromised, where the argument expression can itself fail (compounding a
  second failure onto an in-flight one) or block (a channel send at exit
  with no receiver). Registration evaluates arguments on the normal path,
  where the state they were written against still holds.
- **Predictability matches the language.** `gof` is immutability-first and
  explicit; the deferred action depending only on registration-time state is
  the non-magical reading. Go ships this semantics for the same reasons.
- **RFC 0043 composability.** Argument handling is exactly ordinary
  call-argument handling — value types duplicate (O(1) CoW), handles share
  the resource (precisely what `defer close(handle)` needs), owned types
  move at registration and misuse is caught statically by `GOF3129`.

```gof
fn audit() -> unit:
    mut stage = "connect"
    defer print(stage)
    stage = "handshake"
    print(stage)      # prints "handshake" now
    ...               # defer prints "connect" at scope exit — the registered snapshot
```

### Deferred call results and failures

The deferred call's return value is discarded by the `defer` statement. The
explicit `defer` keyword is the explicit intent that the result is not
handled inline. Failures follow RFC 0043's fallible-destruction rule —
reported, never swallowed, never blocking the rest of cleanup:

- a deferred call completing with `Result.Ok` (or any non-`Result` value,
  including `unit`) has no observable effect beyond the call itself
- a deferred call completing with `Result.Err` surfaces as runtime
  diagnostic `GOF3138` located at the `defer` registration site, with the
  rendered error in the message
- a deferred call raising its own runtime diagnostic keeps that diagnostic
  as primary (its own code and span) with a `GOF3138` note identifying the
  defer registration and the scope-exit execution
- in both failure cases the remaining deferred calls and the destruction
  sequence still run; collected failures are reported together once the
  scope finishes exiting

Interaction with the exit class:

- **normal exit**: collected defer failures become the exit's runtime
  diagnostics and the exit is a failed exit — after all cleanup completes
- **`?` exit**: the deferred failure is primary (`GOF3138`) and the
  propagated `Result.Err` is preserved as a note; the caller does not
  receive a recoverable value when cleanup itself broke
- **runtime-diagnostic exit**: the original diagnostic stays primary;
  deferred failures attach as notes/secondary diagnostics

The `?` rule is deliberate: at scope exit there is no live caller context
that can receive a `Result` from cleanup — multiple defers cannot all become
the function's return value — so cleanup failures are destruction-class
(RFC 0043 makes destruction failures diagnostic-class by construction), and
the original recoverable error stays visible in the note rather than being
dropped, which is exactly the defect Go's defer exhibits (silently dropping
`Close()` errors). When a close failure is *expected and recoverable*, the
idiom is to close explicitly on the success path — `close()` is idempotent
per RFC 0043, so the deferred close remains a safety net, not a double-fault:

```gof
fn write_report(path: string, payload: string) -> Result[unit, RuntimeError]:
    stream = open_write_stream(path)?
    defer stream.close()                    # safety net for `?` and diagnostic exits
    stream.write_all_string(payload)?       # recoverable: caller handles; stream still closes
    stream.close()?                         # typed handling when close failure is expected
```

### No recover

Deferred calls cannot cancel, replace, or observe an in-flight exit beyond
running. There is no `recover`: panics are for broken invariants (AGENTS.md
§5) and recoverable errors are typed `Result` values; a recover mechanism
would open a second, dynamic error channel and make scope-exit behavior
unpredictable. The exit that triggered unwinding always completes as it
started, after defers run.

### Task and concurrency interaction

- `defer` inside a function spawned with `go` runs at that task function's
  exit; task-boundary conversion to `TaskFailed` / `TaskPanicked` happens
  after the defers have run, so panicking tasks still clean up
- a deferred call blocks exactly as the same call would anywhere —
  `defer send(ch, value)` on a full channel blocks at scope exit; the
  cancellable form `defer send(ch, value, token)` keeps cancellation
  explicit and token-driven (the shipped cooperative-cancellation story)
- defer introduces no new shared state: handle arguments are the RFC 0043
  synchronized handles, value arguments are thread-owned duplicates, and the
  defer record itself is function-local — no new race surface exists

### Guardrail answers (AGENTS.md §5)

- **who owns the data**: the block activation's defer record owns the
  resolved target and evaluated arguments; value arguments are CoW
  duplicates, handle arguments share the synchronized resource, owned
  arguments move in at registration
- **where values live**: in the defer record attached to the block
  activation — fixed stack slots in the native frame, evaluator values in
  the block's pending list
- **where allocation happens**: at the `defer` statement, once per
  execution; record capacity is statically bounded by the number of defer
  sites in the block, so the native target needs no heap allocation for
  defer itself, and argument cost is the ordinary call-argument cost
- **where blocking can happen**: only inside the deferred call, at scope
  exit, with the call's own semantics; defer never introduces blocking the
  call would not already have
- **how races are prevented**: no new sharing; handle args are synchronized
  by RFC 0043 construction, value args are thread-owned, the record is
  function-local
- **how the feature is debugged**: deterministic LIFO order; `GOF3138`
  carries the registration span and the rendered failure; failures are
  collected, ordered, and reported after cleanup completes

### Layer implementation notes

- **lexer**: no change — `TokenKind::Defer` exists
- **CST/AST**: `parse_stmt` gains a `Defer` branch parsing `defer` +
  expression + newline into `Stmt::Defer { call, span }`; `defer` at module
  top level stays rejected by the existing module-level declaration rule;
  the formatter emits `defer ` + expression
- **HIR**: `HirStmt::Defer` mirrors the statement, one-way lowering
- **typed HIR**: defer-contract checks — target must be a resolved call
  (`GOF3136`), no postfix `?` on the target (`GOF3137`); target arity and
  argument types use the existing call diagnostics; the move checker treats
  defer arguments as consumed at the `defer` statement
- **MIR/SSA/native**: each block with defer sites gets a scope-exit region;
  defer slots form a fixed-size array indexed by registration order; all
  exit edges (fallthrough, `return`, propagate, diagnostic-unwind landing
  pad, `break`, `continue`) route through the region — defers first, then
  destruction
- **interpreter**: block activations carry a pending-defer list; every exit
  path (`EvalOutcome` variants and `EvalSignal::Propagate` /
  `EvalSignal::Diagnostics`) flushes the list LIFO before the exit
  propagates, collecting failures per the failure classes above

## Alternatives Considered

### Function-scoped defer (Go)

Rejected. Defer attaching to the function rather than the block is the
documented Go loop footgun — resources held until function exit, unbounded
defer-stack growth per iteration, wrapper functions needed for per-iteration
release — and it contradicts `gof`'s block-scoped destruction (RFC 0043),
since a function-scoped defer outlives the scope owning its arguments.
Block scoping is uniform, ownership-coherent, and statically bounded.

### Defer blocks evaluated at exit (Zig / Swift)

Rejected for v1. `defer:` blocks would require closures (explicit capture
lowering is still a Phase 1 RFC) and would read mutable state at scope exit
— exit-time re-evaluation, the exact dynamic behavior this RFC refuses. The
call-with-registration-time-arguments form is fully expressible today. A
block form remains a compatible future extension once closures exist and a
real need is demonstrated; nothing in this RFC's scoping or ordering
forecloses it.

### Lazy argument evaluation at scope exit

Rejected. Re-evaluated arguments run against unwinding state: a `mut`
rebinding silently redirects cleanup, the argument expression can fail or
block while the program is already failing, and the visible cost moves from
the acquisition point to every exit path. Registration-time evaluation is
the predictable, Go-proven choice.

### try/finally blocks (Java / Python / C# / JS)

Rejected. A structural control-flow tax: new nesting around every acquired
resource, `finally` runs even on paths that never acquired anything, and
the `finally` family has its own defect class (a `return` inside `finally`
swallowing the in-flight exception). `defer` registers at the acquisition
point and composes with existing blocks — no new nesting, no new exit
semantics.

### RAII-only (C++ / Rust Drop)

Rejected as the sole mechanism. RFC 0043 has no user-declared destructors,
handle release is runtime-driven final-reference close, and most deferred
work is not destruction at all (logging, `send`, unlock, counters). C++
RAII requires a wrapper type per action. Defer is the explicit user-side
primitive on top of the deterministic-destruction base — the combination
VISION.md names as the model ("deterministic resource release (`defer`,
final-reference close)").

### Go-style recover

Rejected. No recover, ever: panics are broken invariants (AGENTS.md §5),
recoverable errors are typed `Result` values, and recover would turn panics
into a second dynamic error channel and make scope-exit ordering
unpredictable. Defer cleans up; it never cancels an exit.

### Restrict defer targets to unit-returning calls

Rejected. Without closures there is no way to wrap the fallible `close()`
family, so `defer close(stream)` — the primary idiom this feature exists
for — would be illegal. Result discipline is preserved instead by the
`GOF3138` destruction-class failure reporting and RFC 0043's idempotent
`close()`, with explicit inline `close()?` when the failure is expected and
recoverable.

## Compatibility Impact

- **Pure addition.** `defer` is a reserved token and a spec-v1 surface
  entry; every current use is a parse error, so no shipped program or
  example can change behavior. No existing statement, expression, or
  diagnostic semantics are altered.
- **RFC 0043 / ADR 0003**: this RFC supplies the `defer` half of the already
  accepted "defers first, then destruction" ordering; implementation shares
  the scope-exit machinery, and the ordering conformance tests land with
  whichever slice lands second.
- **Runner/test lifecycle**: defer in a `test fn` body runs at body exit,
  before runner `TestContext` temp/env teardown; defer in a `fixture` body
  runs at fixture-function exit, before the value is cached. RFC 0042's
  `cleanup()` hooks remain the runner-level teardown protocol — no overlap,
  no reordering.
- **VS Code extension**: the grammar already highlights `defer` as a keyword
  (`tools/vscode-gof/syntaxes/gof.tmLanguage.json`), so the only
  extension-visible change is a snippet; `npm test` and `npm run package`
  are the verification per AGENTS.md §9.
- **Docs/spec sync when implemented**: `spec/language-v1.md` gains the defer
  semantics bullets, `spec/diagnostics.md` gains `GOF3136`–`GOF3138`,
  `docs/site/content/docs/en/control-flow.mdx` and the language tour gain a
  defer section, `examples/` gains a defer walkthrough, and `roadmap.md`
  moves defer out of the token-only state.

## Diagnostics Impact

Numbering: `GOF3128` is the last shipped code; RFC 0043 reserved
`GOF3129`–`GOF3131`. This RFC takes `GOF3136`–`GOF3138` only and allocates
nothing in `GOF3132`–`GOF3135`.

All three follow the diagnostics contract (code / severity / span / message
/ note / fix-it):

- `GOF3136` (error): invalid `defer` target — the statement's expression is
  not a call expression, or resolves to a struct constructor or enum payload
  construction. Span: the target expression. Note: states what was found and
  that `defer` requires a call. Fix-it: best-effort `(...)` append when the
  target is a bare method or function reference.
- `GOF3137` (error): postfix `?` applied to the deferred call —
  `defer send(ch, value)?`. Span: the `?` postfix. Note: a deferred call
  runs at scope exit where there is no propagation destination; its result
  is discarded and failures surface as `GOF3138`. Fix-it: remove the `?`.
- `GOF3138` (error, runtime): deferred call failed at scope exit — a
  deferred call completed with `Result.Err` or raised a runtime diagnostic
  while the scope was exiting. Span: the `defer` registration site. Message:
  rendered call and rendered failure. Note: remaining deferred calls and
  destruction completed before reporting; on a `?` exit the propagated error
  is preserved in the note; on a diagnostic exit the original diagnostic
  stays primary.

Reused, unchanged: ordinary call arity/type diagnostics for the target;
`GOF3129` for use-after-move of owned defer arguments; the existing
module-level declaration rule rejects top-level `defer` with no new code.
`spec/diagnostics.md` gains the three codes when the implementation lands.

## Test Plan

- **parser**: positive fixtures for every valid target form (top-level
  function call, method call, builtin call); malformed forms (missing
  expression, trailing tokens after the call)
- **typed HIR**: negative fixtures for `GOF3136` (identifier, literal,
  binary expression, field access, bare method reference, struct
  constructor, enum payload construction) and `GOF3137` (`defer f(x)?`,
  `defer value.method(x)?`); positive fixture: `Result`-returning target
  compiles
- **interpreter — ordering**: LIFO within a scope; per-scope stacks with
  inner-scope unwind (the `arm`/`function` example); loop-body per-iteration
  flush including `break` and `continue`; `?` exit; runtime-diagnostic exit
  (e.g. division by zero); `return` from nested blocks; registration-time
  argument snapshot under `mut` rebinding; handle sharing
- **interpreter — failures**: `Result.Ok` / `unit` discard; `Result.Err`
  completion raising `GOF3138` at the registration span; deferred call
  raising its own diagnostic (original primary, `GOF3138` note); failure
  continuation (remaining defers and destruction still run); collected
  multi-failure reporting; propagated-Err-preserved-as-note on `?` exits;
  original-diagnostic-primary on diagnostic exits
- **tasks**: defer in a `go`-spawned function on the success path and the
  panic path (`TaskPanicked` conversion happens after defers)
- **test platform**: defer in `test fn` bodies runs before `TestContext`
  temp/env teardown; defer in `fixture` bodies runs before the fixture value
  is cached
- **conformance**: identical observable behavior under the bootstrap
  evaluator and (Phase 2) the native target for the ordering suites;
  defer-before-destruction ordering tests land jointly with RFC 0043's
  destruction implementation
- **docs/extension**: defer example in `examples/` with README entry;
  control-flow docs; VS Code snippet with `npm test` and `npm run package`

## Benchmark Impact

No zero-cost claim is accepted without a number (VISION.md honesty rules);
the gates this RFC sets for Phase 2:

- **defer-free functions pay nothing**: scopes without defer sites have no
  exit-region code emitted — measured on the existing benchmark corpus
- **registration cost** = argument evaluation + one slot store, at the
  acquisition point; no hidden exit-time argument work
- **per-scope storage is statically bounded** by the number of defer sites
  in the block — no heap allocation for the defer record, no per-iteration
  growth (the Go defect); verified by codegen inspection
- **scope-exit overhead** for scopes with defer sites: one guard check per
  exit edge; measured on hot-loop benchmarks so cleanup-heavy loops stay
  predictable
- the bootstrap evaluator is a conformance oracle only — no performance
  claim is made against it
