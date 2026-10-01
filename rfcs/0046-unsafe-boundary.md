# RFC 0046: Unsafe Boundary

## Summary

Define the `unsafe` boundary for `gof`: a **block statement** — `unsafe:`
followed by an indented body — marking the only region where explicitly
unsafe operations may appear.

This RFC lands the boundary and its rules only. It adds **no unsafe
operation**: raw pointers, layout casts, FFI, and ABI control are Phase 2+
work (native backend first, FFI/ABI after it, per the VISION phases) and
each arrives in its own RFC against this fixed frame.

Four boundary rules, fixed now:

1. **explicit only** — unsafe is never inferred, implicit, or
   compiler-inserted; every explicitly unsafe operation requires a
   lexically enclosing `unsafe:` block
2. **full typechecking inside** — `unsafe:` grants permission, never
   relaxations; every static contract (types, immutability, exhaustiveness,
   RFC 0043 ownership) applies unchanged
3. **no signature leak** — no `unsafe fn`, no unsafe methods, no unsafe
   modules; unsafety never appears in a signature and is never viral
4. **lexical, function-local context** — the unsafe context is a property
   of source text, stops at function boundaries, and is never created by
   dynamic execution

Plus one documented obligation: the **safe-wrapper requirement** — every
`unsafe:` block lives inside a function whose public contract is safe.

`unsafe` already lexes as a keyword and is listed in the spec v1 syntax
surface. In v1 no unsafe operation exists, so every `unsafe:` block is
semantically a plain block; diagnostic `GOF3141` makes that honesty visible
instead of letting the keyword pretend to do something.

## Problem

Three documents already promise this feature:

- AGENTS.md §5: "unsafe/FFI only behind explicit safe boundaries"
- VISION.md anti-defect targets: "no undefined behavior; memory unsafety
  only behind explicit `unsafe`"
- spec v1 surface lists `unsafe`, and the lexer already reserves the
  keyword (`TokenKind::Unsafe`), which blocks it as an identifier

None of them define what the boundary *is*. Today `unsafe` is
reserved-vocabulary debt: a token that cannot be used and has no meaning.
The spec's closing rule — future work "intentionally blocked from
pretending to be stable" — is exactly the state the keyword is in.

The boundary must be decided **before** the operations, for the same reason
RFC 0043 fixed ownership before backend design:

- Phase 0 explicitly lists the unsafe RFC among the core RFC drafts; Phase
  2 (native backend) and the FFI/ABI work after it need a fixed rule frame
  so each operation RFC extends it instead of inventing boundary semantics
  under deadline pressure
- the compiler layers need the shape once: parser statement form, HIR
  marker, typed-HIR context flag, diagnostics vocabulary — retrofitting
  these after the first FFI RFC would force that RFC to design both the
  operations and the boundary at once
- without a fixed boundary, "memory unsafety only behind explicit unsafe"
  is a marketing claim; with one, it is a checkable property

AGENTS.md §5 asks of any new feature: who owns the data, where values
live, where allocation happens, where blocking can happen, how races are
prevented, how it is debugged. The boundary itself changes no runtime
behavior, so those questions are inherited unchanged — but the rules below
force every future unsafe-operation RFC to answer them explicitly.

This RFC is language-surface only. No ADR is required because no runtime,
codegen, or ABI behavior changes in v1.

## Proposed Change

### Syntax and scoping

```gof
unsafe:
    <statements>
```

- the same shape as every other gof block statement: `unsafe` keyword,
  `:`, newline, `INDENT`, statements, `DEDENT`
- valid wherever a statement is valid: `fn`, `test fn`, and fixture
  bodies, and inside `if` / `while` / `for` / `match` arm / `select` arm
  bodies
- it is a **statement, not a modifier**: `unsafe` cannot decorate `fn`,
  `struct`, `enum`, `protocol`, or any other statement or declaration
  (`GOF3140`)
- it opens a new lexical scope: bindings created inside follow RFC 0043
  ownership rules and are destroyed at block exit in reverse creation
  order, exactly as in a plain block
- control flow is ordinary: `return`, `break`, `continue`, and `expr?`
  inside an `unsafe:` block target the enclosing function or loop exactly
  as in a plain block
- nesting `unsafe:` inside `unsafe:` is legal and redundant; there is no
  nesting error and no nesting depth limit

Illustrative wrapper shape (the operation names are Phase 2+ surface and
do not exist yet):

```gof
fn read_header(raw: Bytes) -> Result[int, RuntimeError]:
    # preconditions validated in safe code, before the block
    if raw.len() < 4:
        return Result.Err(RuntimeError.Slice("header too short"))
    header = 0
    unsafe:
        # Phase 2+ explicitly unsafe operations go here, and only here
        header = decode_header_layout(raw)
    return Result.Ok(header)
```

### What `unsafe:` means — and does not mean

In v1, `unsafe:` grants exactly one thing: **permission**. Statements
inside the block may, in later phases, use explicitly unsafe operations.
It grants nothing else:

- zero type-system relaxations
- zero ownership or mutability relaxations — RFC 0043 checks
  (`GOF3129`, `GOF3130`, `GOF3131`) fire identically inside `unsafe:`
  blocks
- zero optimizer licenses — the marker is not a codegen input in v1 and
  the optimizer derives nothing from it
- zero concurrency exemptions — channels, tasks, and `select` rules are
  unchanged

Because no unsafe operation exists in v1, an `unsafe:` block checks,
lowers, and runs identically to a plain block. The only differences are
the marker in the tree and the `GOF3141` warning. No shipped builtin,
module, stdlib surface, or example needs or gains `unsafe`, ever: if an
operation can be safe, it must be safe.

### Rule 1 — unsafe is explicit only

Every explicitly unsafe operation requires a **lexically enclosing
`unsafe:` block** at its syntactic position. No operation is unsafe
implicitly, contextually, or by inference:

- context never upgrades an operation to unsafe
- the compiler never inserts an unsafe operation
- the compiler never derives an optimization assumption ("treat this as
  well-defined") from the presence or absence of `unsafe:`
- the unsafe-operation list is closed: an operation is unsafe exactly
  when a published RFC says so, in that RFC's own text
- there is no build flag, module attribute, or edition that licenses
  unsafe operations without the block

### Rule 2 — typechecking applies in full

`unsafe:` is a permission region, not an escape hatch. All static
contracts — typing, immutability, exhaustiveness, and RFC 0043
ownership/move/destruction rules — apply inside `unsafe:` blocks exactly
as outside. This rule is deliberately stated now, before any operation
exists, to close off the future temptation to let `unsafe:` silence the
move checker or relax exhaustiveness. `unsafe:` must never become "the
check-suppression keyword".

### Rule 3 — no signature leak

There is no `unsafe fn`, no unsafe method, no unsafe protocol
implementation, and no unsafe module — not in v1, and not in this design
ever. Consequently:

- a function containing an `unsafe:` block has an ordinary signature;
  callers cannot detect unsafety from the signature and need no ceremony
  to call it
- unsafety never propagates up a call graph: calling a function that
  contains an unsafe block does not put the caller in unsafe context
- unsafety never crosses imports: a module that uses `unsafe:` does not
  make importing modules unsafe

Because the signature cannot carry the warning, the obligation lands on
the function author — that is the safe-wrapper requirement below.

### Rule 4 — lexical, function-local context

The unsafe context is a property of source text, not of runtime behavior:

- a position is in unsafe context if and only if it is inside an
  `unsafe:` block in the **same function body**
- the context does not cross function boundaries: nested functions,
  closures (Phase 1, RFC pending), and fixture or test bodies declared
  textually inside an `unsafe:` block do not inherit it; their bodies
  must open their own `unsafe:` blocks
- the context is not created or extended by dynamic execution: deferred
  work and late-run closures do not execute "in" a context they did not
  lexically have. The `defer` RFC (pending) and the closures RFC (pending)
  pin their exact wording against this principle

### Safe-wrapper requirement

Normative requirement. v1 enforcement is documentation and review;
machine enforcement is deferred to the phase that ships the first unsafe
operations.

Every `unsafe:` block must live inside a function whose public contract
is safe. The wrapper must:

- validate preconditions in ordinary safe code **before** the block
- keep the block minimal: whole statements, narrow extent, nothing
  inside that can live outside
- surface failure as `Result[T, E]` per the errors contract; `panic`
  only for broken invariants
- expose no unsafe construct (raw pointer, layout view, FFI handle)
  through its signature
- document the invariant that must hold for the operations inside to be
  sound — the thing a reviewer checks

Phase 2+ may add a lint requiring a `# safety:` comment inside each
`unsafe:` block and a wrapper-size lint. This RFC commits to the
requirement, not the lint. `GOF3141`'s note points here.

### Pipeline placement

- lexer: no change — `TokenKind::Unsafe` already exists
- parser / CST / AST: new statement form `UnsafeBlock { body }`
- HIR: a block node with an `unsafe` marker; no semantics
- typed HIR: the checker threads an unsafe-context flag through
  statement checking; this is the enforcement point for `GOF3139` and
  the computation site for `GOF3141`
- MIR / SSA / backend: no change in v1; the marker is not a lowering
  input until Phase 2 RFCs define operations and their lowering
- evaluator: executes the block as a plain block

The context flag is per statement position, not per expression: rule 4
needs only block extents, so typed-HIR cost is O(number of statements)
with a small constant.

### Forward hooks pinned now (Phase 2+)

- `GOF3139` vocabulary is reserved so the first operation RFC (raw
  pointers, layout casts, FFI — Phase 2 native backend, then FFI/ABI)
  plugs into a fixed diagnostic
- operation RFCs must declare each operation unsafe explicitly in their
  own text; none may redefine the boundary rules
- unsafe operations must respect the concurrency contract (channels,
  tasks, cancellation) or amend it explicitly in their own RFC
- a future `gof build` flag may deny `unsafe:` blocks outright for
  teams that want a hard no-unsafe guarantee; deferred
- unsafe-block count per module becomes an audit metric from the day
  operations exist

## Alternatives Considered

### Rust-style fn-level `unsafe fn` with caller-side `unsafe` blocks

In Rust, `unsafe fn` licenses the whole function body for unsafe
operations, and callers must invoke it inside `unsafe { }` blocks (or be
`unsafe fn` themselves). The 2024 edition's `unsafe_op_in_unsafe_fn` lint
pushes Rust toward also requiring inner blocks.

Rejected for gof:

- viral signatures: unsafety becomes part of the callable's contract and
  propagates through call graphs — the same ceremony tax RFC 0043
  rejected for lifetimes, arriving by a different door
- the license sits far from the operation: a 50-line `unsafe fn`
  licenses line 3 and line 50 equally; a block keeps the licensed region
  minimal, greppable, and reviewable at the exact statements that need it
- caller-side ceremony multiplies noise at every call site instead of
  concentrating the proof obligation in one wrapper
- the dual surface (fn-level plus block-level) is redundant; gof's
  priority order puts surface-area growth last, and one mechanism is
  coherent where two are not
- Rust's own 2024 migration is evidence that fn-level alone proved too
  coarse; gof starts at that endpoint

Conceded tradeoff: fn-level gives a named, reusable, documentable
boundary. gof gets the same thing through ordinary functions — the
safe-wrapper requirement puts the `unsafe:` block inside a helper with a
documented invariant, which is exactly that named boundary, minus the
viral signature. That is the intended discipline, not a loss.

### Inline unsafe expressions (`unsafe expr`)

Rejected. Statement-granularity blocks match gof's indentation-block
language shape; an inline expression form invites sprawling one-line
licensed regions, interacts with operator precedence, and gives
precondition checks and safety comments no natural home adjacent to the
licensed statements.

### Module-level unsafe opt-in

Rejected. The coarsest possible boundary: an entire module silently
licensed, contradicting AGENTS.md §5's "explicit safe boundaries" and
making the review scope unbounded. It is also viral by construction,
which rule 3 forbids.

### No `unsafe` at all (pure safe language)

Rejected. VISION fixes the target — "memory unsafety only behind explicit
`unsafe`" — so the escape hatch is the promise, not the hole. Phase 2+
FFI/ABI and layout control cannot be expressed without it, the runtime
and future `Box[T]` internals need an implementation language, and the
token is already reserved in the v1 surface, blocking the identifier
anyway.

### Defer the boundary until the first operation exists (Phase 2)

Rejected. VISION Phase 0 lists the unsafe RFC among the core RFC drafts
precisely so operation RFCs extend a fixed frame. Landing the statement
form, the typed-HIR context flag, and the diagnostics vocabulary now
means the first FFI/pointer/layout RFC carries only operations. Waiting
invites the worst outcome: an operation RFC shipping ad hoc boundary
rules under Phase 2 pressure.

## Compatibility Impact

- `unsafe` already lexes as a reserved keyword and is already listed in
  the spec v1 syntax surface, so no valid program uses it as an
  identifier; the new statement form cannot break existing programs
- purely additive: programs that do not write `unsafe:` compile
  identically with and without this feature
- no shipped builtin, module, stdlib surface, or existing example gains
  or requires `unsafe`
- evaluator and native parity in v1: an `unsafe:` block runs identically
  to a plain block, so conformance-observable behavior is unchanged
- RFC 0043 interactions unchanged: ownership, move, and destruction rules
  apply inside `unsafe:` blocks exactly as outside
- VS Code extension: grammar gains the `unsafe:` block form; a snippet
  for the safe-wrapper shape; `npm test` and `npm run package` verified
  per AGENTS.md §9

## Diagnostics Impact

`GOF3129`–`GOF3131` remain reserved by RFC 0043 and `GOF3132`–`GOF3138`
remain reserved by the in-flight RFCs that claimed them. This RFC adds
exactly three codes, all following the diagnostics contract
(code/severity/span/message/note/fix-it):

- `GOF3139` (error): explicitly-unsafe operation used outside a
  lexically enclosing `unsafe:` block. Span: the operation. Note: names
  the enclosing function and states that unsafe context never crosses
  function or declaration boundaries. Fix-it: wrap the statement in an
  `unsafe:` block. Reserved by this RFC; first emits when the first
  explicitly-unsafe operation lands (Phase 2+).
- `GOF3140` (error): `unsafe` used as a declaration or statement
  modifier (for example `unsafe fn`, `unsafe struct`, `unsafe` prefixing
  a plain statement or top-level declaration). Message: `unsafe` is a
  block statement in gof and never part of a signature. Fix-it: remove
  the `unsafe` modifier, leaving the valid declaration or statement.
- `GOF3141` (warning): `unsafe:` block is inert — it transitively
  contains no explicitly-unsafe operation. In v1 every `unsafe:` block is
  inert because no unsafe operations exist; from Phase 2+ it fires only
  on genuinely useless blocks. Fix-it: remove the `unsafe:` header line.
  Note: points to the safe-wrapper requirement.

`spec/diagnostics.md` gains all three codes when the parser and typed-HIR
slice lands.

## Test Plan

- parser positive fixtures: `unsafe:` blocks in `fn`, `test fn`, and
  fixture bodies; nested inside `if` / `while` / `for` / `match` arm
  bodies; nested inside another `unsafe:` block; bindings scoped to the
  block
- parser negative fixtures: `unsafe fn`, `unsafe struct`, and `unsafe`
  prefixing a statement or top-level declaration report `GOF3140`;
  malformed block shapes (missing `:` or indented body) reuse the
  existing block-statement diagnostics
- typed-HIR tests: unsafe-context flag threading across statement forms;
  RFC 0043 checks unchanged inside unsafe blocks — a negative fixture
  where a use-after-move inside an `unsafe:` block still reports
  `GOF3129`; inert-block detection reports `GOF3141` with its fix-it
- evaluator conformance: `unsafe:` blocks execute identically to plain
  blocks; RFC 0043 scope-exit destruction tests re-run with
  unsafe-wrapped scopes
- CLI conformance: `gof run` and `gof build` on programs containing
  `unsafe:` blocks
- docs/spec synchronization (AGENTS.md §8): spec v1 gains the boundary
  rules under the existing `unsafe` surface entry; diagnostics registry
  updated; an `examples/` file demonstrates the wrapper shape and
  documents the Phase 2+ operation surface in comments
- VS Code extension (AGENTS.md §9): grammar and snippet updated;
  `npm test` and `npm run package` verified

## Benchmark Impact

- runtime: no code is generated for the boundary in v1 — `unsafe:`
  blocks lower identically to plain blocks, so there is no zero-cost
  claim to benchmark
- compile time: one statement form and one per-statement context flag;
  expected to be unmeasurable (O(number of statements), small
  constant); if typed-HIR timing regresses, the flag is the first
  suspect
- Phase 2+ gates, recorded then and not claimed now: the codegen delta
  of actual unsafe operations; unsafe-block count per module as an audit
  metric
