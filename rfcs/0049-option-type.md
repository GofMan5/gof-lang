# RFC 0049: Option Type

## Summary

Add typed absence to `gof` through a builtin parameterized enum, mirroring the
shape of `Result[T, E]` exactly:

- `Option[T]` — builtin parameterized type with exactly one type parameter
- `Option.Some(value)` and `Option.None` — builtin constructors, where
  `Some` is a one-payload variant and `None` is a unit variant
- exhaustive statement-level `match` over `Option` with the same rules as
  `match` over `Result`
- a minimal explicit helper set: `unwrap_or(default)`, `is_some()`,
  `is_none()` as receiver-style calls on `Option` values
- no implicit coercion between `T` and `Option[T]` in either direction
- postfix `?` stays `Result`-only in v1
- sequence helpers `first` / `last` / `min` / `max` keep their
  `Result[element, RuntimeError]` contracts; no stdlib migration in this RFC

This closes the VISION.md anti-defect target "no null dereferences — absence
is `Option[T]`, failure is `Result[T, E]`" and is the Phase 0 core RFC draft
for Option, with implementation landing in the Phase 1 semantic core.

## Problem

`gof` has no null — and no `Option` either. The language promises that
absence is a typed value, but today a "maybe there is no value" API can only
encode absence in one of three ways, each with a defect:

- **sentinel values** (`0`, `""`, `-1`, an empty list): the sentinel is
  indistinguishable from a legitimate value. Every call site re-implements a
  private convention, and the classic null-dereference defect class survives
  under a different name.
- **a fabricated `Result[T, RuntimeError]`**: the caller invents an error
  value for a condition that is not a failure. This smears the absence
  channel into the failure channel, forcing fake messages
  (`RuntimeError.EmptySequence("first of empty list")`) and making every
  reader ask "did the operation fail, or did nothing match?".
- **ad-hoc user enums**: each project declares its own
  `enum MaybePort: Found(value) | Missing` shape. With no user-defined
  generics yet (RFC 0015 non-goal), every such enum is monomorphic, none of
  them interoperate, and the language gains no shared absence vocabulary.

The channel distinction is a readability and correctness contract:
`Result[T, E]` answers "the operation failed, here is why"; `Option[T]`
answers "there was no value to give you". VISION.md pins this split as an
anti-defect target, and the roadmap Phase 0/1 program requires the RFC and
the semantics before the semantic core freezes. Every future lookup-shaped
stdlib API (dict lookup, `find`, optional config fields) is currently
blocked on this decision: it either waits for `Option` or picks one of the
three defective encodings and cements it.

## Proposed Change

### Type, constructors, and annotation positions

- `Option[T]` is a builtin parameterized type with exactly one type
  parameter.
- `Option.Some(value)` constructs the payload variant with exactly one
  payload of type `T`.
- `Option.None` is the unit variant, written without parentheses, exactly
  like a unit enum variant reference (`EnumName.Variant`).
- Variant references require the qualified `Option.` prefix. `Some` and
  `None` are not reserved words; unqualified `Some` / `None` remain ordinary
  identifiers and fail with the existing unknown-binding diagnostic.
- `Option[T]` is accepted in every annotation position that accepts
  `Result[T, E]` today: function parameters, explicit return types, and
  typed local bindings. Nesting (`Option[Option[T]]`, positions inside other
  parameterized annotations) follows the same support level the type layer
  provides for `Result` nesting.
- Annotation arity violations (`Option[int, string]`) reuse the existing
  parameterized-type arity diagnostic.
- Return-type inference mirrors `Result`: an unannotated function whose
  returns are `Option.Some(1)` infers `Option[int]`, and every `return` in
  one function must resolve to one compatible type.
- Equality mirrors `Result`: two `Option[T]` values are equal when both are
  `None`, or both are `Some` with equal payloads, and only when `T` itself
  has explicit equality semantics. `Option` values never participate in
  ordering.
- Rendering follows the existing enum-shaped convention used by `Result`
  values: `Option.None` for the unit variant and
  `Option.Some(value: <payload>)` for the payload variant, with the exact
  format pinned by conformance snapshots.

```gof
fn find_arg(args: list[string], prefix: string) -> Option[string]:
    for arg in args:
        if starts_with(arg, prefix):
            return Option.Some(arg)
    return Option.None

fn port_from_args() -> Result[int, RuntimeError]:
    arg = find_arg(argv(), "--port=")
    match arg:
        Option.Some(value):
            parts = split(value, "=")
            return parse_int(parts[1])
        Option.None:
            return Result.Err(RuntimeError.EnvMissing("port"))
```

This pair is the canonical shape of the two-channel contract: `find_arg`
answers a query (a flag may be absent, so it returns `Option[string]`);
`port_from_args` performs an operation (parsing may fail, so it returns
`Result[int, RuntimeError]`); and the explicit `match` is where the one
deliberate channel crossing happens — absence becomes a typed operational
error chosen by the caller, not by a hidden conversion.

The helpers cover the cases that do not deserve a full `match`:

```gof
retries = parse_int(find_arg(argv(), "--retries=").unwrap_or("3"))
verbose = find_arg(argv(), "--verbose=").is_some()
```

### Exhaustive `match`

`match` over an `Option` value works exactly like `match` over a `Result`:

```gof
match find_arg(argv(), "--port="):
    Option.Some(arg):
        print(arg)
    Option.None:
        print("no port argument")
```

Rules, mirroring the `Result` match contract rule for rule:

- `match` remains statement-level; it is not an expression.
- Exhaustiveness requires exactly the `Option.Some(...)` and `Option.None`
  arms, and only those arms.
- Each variant may appear at most once in one `match`.
- `Option.Some(binding)` binds exactly one payload.
- `Option.None` takes no binding.
- Arms from the wrong namespace (`Result.Ok(x)` against an `Option`
  scrutinee) are rejected by the existing match-arm validation.

Because `Option` lowers through the same enum-shaped payload machinery as
`Result`, the exhaustiveness, duplicate-arm, and payload-binding checks are
the existing checks; `Option` is a new fixed inhabitant of that machinery,
not a new match subsystem.

### Ownership interaction (RFC 0043)

`Option[T]` joins the RFC 0043 composites and takes its duplication behavior
from its payload, exactly as `Result[T, E]` does:

- `Option` of a value type is a value type: `Copy` with copy-on-write.
  Matching never consumes a value-type scrutinee.
- `Option` of a handle type (`Option[channel[int]]`) shares the underlying
  resource, like any composite holding a handle field.
- `Option` of an owned type (`Option[Box[T]]` when `Box` lands) is
  move-only where RFC 0043 says so: a `match` payload binding that moves an
  owned payload consumes the scrutinee binding.
- Task-boundary rules for `Option` payloads crossing `go` / `send` /
  `await` are the RFC 0043 rules for composites, unchanged.

### The helper surface

Three helpers, as receiver-style calls on `Option` values:

| call | returns | behavior |
|---|---|---|
| `opt.unwrap_or(default)` | `T` | `Some(v)` yields `v`; `None` yields `default` |
| `opt.is_some()` | `bool` | `true` for `Some`, `false` for `None` |
| `opt.is_none()` | `bool` | `true` for `None`, `false` for `Some` |

Contracts:

- `unwrap_or` requires the receiver to be `Option[T]` and `default` to be
  compatible with `T`; it is total — it never panics and never returns
  early.
- `is_some` / `is_none` require the receiver to be `Option[T]` and take no
  arguments.
- All three are pure: no receiver mutation, no allocation beyond the result.
- Receiver-call syntax mirrors the shipped builtin-receiver precedent
  (`Bytes.len()`, `TestContext.ok(...)`) instead of adding new global
  builtin names, which user top-level functions can shadow today.

Why exactly these three, and nothing more:

- `match` is statement-level, so decoding an `Option` into a value today
  costs a four-line match with early return. `unwrap_or` is the single total
  form that removes the most common ceremony — "absence has an obvious
  static answer" — while keeping the decision visible at the call site.
- `is_some` / `is_none` cover guard, logging, and telemetry conditions.
  Both exist because `not opt.is_some()` is noise, and because an `Option`
  value must never coerce to `bool`.
- Deliberately absent: `unwrap` / `expect` (see Alternatives), the
  higher-order combinators (`map`, `and_then`, `or_else`, `unwrap_or_else`,
  `flatten`, `transpose`, `zip` — closures do not exist in the language
  yet), and the `Option`↔`Result` bridges (`ok_or`, `ok`) — crossing
  channels is a semantic decision that stays explicit in user code via
  `match`.

### No implicit coercion

There is no implicit conversion between `T` and `Option[T]` in either
direction, at any position — annotated bindings, arguments, returns,
payloads, or container elements:

- `port: Option[int] = 8080` is an error; the fix-it wraps the value in
  `Option.Some(8080)`. Wrapping is a semantic decision ("absence is
  possible here") and must be visible.
- Passing `Option[int]` where `int` is required is an error with a
  note-only diagnostic (no fix-it): the safe resolution depends on intent —
  `match`, or `unwrap_or` with a caller-chosen default — and the compiler
  must not guess.
- `Option` values are never truthy: `if opt:` fails the existing
  non-boolean-condition check. `Option` values do not enter arithmetic,
  ordering, `len`, iteration, or indexing.

### Propagation stays `Result`-only

The `expr?` contract is unchanged: the operand must be `Result[T, E]`.
`opt?` fails the existing postfix-`?` operand check, with a note that
`Option` absence is resolved through `match` or `unwrap_or`, not `?`. The
reasoning and the deferral criteria are in Alternatives.

### Scope boundary: no stdlib migration in this RFC

This RFC lands the type, not new APIs on top of it. No shipped builtin
changes its return type; no lookup builtin starts returning `Option` in
v1. Every stdlib adoption (dict lookup, json-null bridging, optional
config fields) is a separate per-API decision in follow-up RFCs, after the
type exists and after usage evidence accumulates.

The forward rule for those decisions: **query-shaped APIs** ("find me the
value for this key, if any") return `Option`; **operation-shaped APIs**
(I/O, parsing, connecting, sending) return `Result` with an operational
error.

## Alternatives Considered

### Postfix `?` on `Option` in v1

Rejected, deferred honestly. In `gof`, `?` means exactly one thing:
propagate an `Err(e)` payload upward. `Option.None` carries no payload, so
`?` on `None` would need one of two semantics, both wrong for v1:

- early-return `Option.None` from an `Option`-returning function — this
  silently unifies absence propagation with error propagation. Callers
  could no longer tell which channel a `?` line is propagating, which is
  precisely what VISION.md's two-channel split exists to prevent.
- convert absence into an error — this requires a user-supplied error value
  (`ok_or(err)?` composition), which needs combinators and closures,
  neither of which exists in the language yet.

Rust's `Try`-trait unification lets `?` silently coerce between `Option`
and `Result` across function boundaries; it is a known foot-gun that `gof`
declines to copy. `?` on `Option` is deferred until closures land, and any
future proposal must weigh it against real usage pain in a dedicated RFC —
not ride in on this one.

### Migrate `first` / `last` / `min` / `max` to `Option` now

Rejected for this RFC; `Result` stays. The honest tension: an empty
sequence has no first element, which is semantically absence, so
`Option[element]` would be the "pure" channel for these helpers. Four
reasons keep the migration out of this RFC:

1. Compatibility: the `Result[element, RuntimeError]` contracts with
   `RuntimeError.EmptySequence(message)` are shipped, tested, and
   documented; migration breaks every caller that matches on
   `Err(EmptySequence(...))`.
2. Information: `EmptySequence` carries a diagnostic message the bare
   `None` of `Option[T]` cannot.
3. Evidence: no usage data shows the migration pays for the churn.
4. Order: this RFC must land the type before a migration target even
   exists.

Recommendation: no migration in this RFC. `Result` stays for operational
errors, including these sequence helpers. If the channel-purity argument
ever wins, the migration is its own likely edition-gated RFC with caller
data — and the same stance applies to the adjacent absence-shaped
operational errors (`env` with `EnvMissing`, `json_get` with missing keys):
documented candidates, not participants in this change.

### `Result[T, unit]` as the absence encoding

Rejected. Mechanically workable where the type layer permits it, and it
reuses existing machinery. But it spends the failure channel on non-failure,
requires a project-local convention every reader must learn, offers no
shared helper vocabulary, and directly contradicts the VISION.md assignment
of absence to `Option[T]`. A language that already committed to
`Result[T, E]` as its error channel owes absence its own first-class name.

### Null references or sentinel defaults

Rejected. This is the defect class `gof` exists to eliminate; VISION.md
lists "no null dereferences" as an anti-defect target. Sentinels are null
semantics with extra steps.

### Nullable sugar syntax (`T?` annotations)

Rejected. It is the same semantics behind new surface: the wrap and unwrap
become less greppable, and `?` already means propagation in postfix
position, so a `T?` type form overloads the symbol in a confusable way.
`Option[T]` keeps construction and destruction explicit and greppable,
which is the point.

### Full combinator set (`map`, `and_then`, `or_else`, `ok_or`, `flatten`, ...)

Deferred. The higher-order combinators need closures, which land with the
Phase 1 closures RFC; shipping their names now would mean shipping
half-designed contracts. The first-class `match` is the primary decoding
tool; combinators get re-evaluated against real usage pain once closures
exist. The `Option`↔`Result` bridges (`ok_or`, `ok`, `transpose`) are
additionally channel-crossing decisions that should stay explicit user
code via `match`.

### `unwrap()` / `expect(message)` accessors

Rejected. A panicking unwrap re-imports the null-dereference defect through
the back door: a missing value becomes a crash the type system just
promised to prevent. `gof` reserves `panic` for broken invariants, and
absence is not a broken invariant — it is an expected state. Exhaustive
`match` and `unwrap_or` are total; no partial panic path is needed.

### User-declared generic enum instead of a builtin

Rejected. User-defined generics do not exist yet, and the absence type is a
cross-module contract: per-project `Maybe` shapes would fragment the
ecosystem exactly where the builtin `Result` shape unified it. Canonical
vocabulary is a language decision.

### Implicit auto-wrap (`T` → `Option[T]`)

Rejected. Invisible wrapping makes every call site ambiguous about which
type it is reading, and the wrap is a semantic statement (absence is
possible here) that must stay visible — mirroring the language-level rule
of no implicit type coercion.

## Compatibility Impact

- Purely additive. `Option` currently names nothing in the language,
  stdlib, examples, or tests — no shipped `.gof` source declares a type or
  function named `Option`.
- Name precedence mirrors the `Result` precedent exactly: the builtin
  `Option` namespace resolves wherever the builtin `Result` namespace
  resolves today (local bindings shadow it the same way, per the existing
  resolution order), and the implementing slice must keep user-declaration
  handling aligned with `Result`: if the type layer rejects user
  declarations that collide with the builtin `Result` namespace, `Option`
  gets the identical treatment with an `Option`-specific note. Any
  divergence between the two is an implementation bug, not a design
  choice.
- No shipped builtin changes its contract; `first` / `last` / `min` / `max`
  and the operational-error family are unchanged.
- New surface: `Option[T]` annotations, two constructors, three receiver
  helpers, and `match` support for the new scrutinee kind.
- Pipeline placement respects the layering contract: the parser treats
  `Option[...]` like any other parameterized annotation (`Result[...]`);
  validation lives in typed HIR; the evaluator reuses the enum-shaped
  value path. No parser-level typing, no new IR kind.
- Per AGENTS.md §8, acceptance requires syncing `spec/language-v1.md`
  (bootstrap subset + binding rules), `spec/diagnostics.md`,
  `roadmap.md` (Phase 0/1 references), and `examples/`.
- Per AGENTS.md §9, the VS Code extension gains grammar and snippets for
  `Option[T]`, `Option.Some`, and `Option.None`, with `npm test` and
  `npm run package` verified.

## Diagnostics Impact

Four new error codes, all following the diagnostics contract
(code/severity/span/message/note/optional fix-it). The last allocated code
before this RFC was `GOF3128`; RFC 0043 reserved `GOF3129`–`GOF3131`; this
RFC allocates `GOF3151`–`GOF3154` and nothing else:

- `GOF3151` (error): invalid builtin `Option` variant reference — an
  unknown or misspelled variant on the `Option` namespace
  (`Option.ok(...)`, `Option.Err(...)` used as an `Option` constructor);
  note lists the valid variants `Option.Some` and `Option.None`.
  Mirrors `GOF3072` for `Result`.
- `GOF3152` (error): invalid payload arity for a builtin `Option` variant
  constructor — `Option.Some()` (missing payload), `Option.Some(a, b)`
  (two payloads), or `Option.None` written with a payload or empty
  parentheses, since the unit variant takes none. Mirrors `GOF3073` for
  `Result`.
- `GOF3153` (error): invalid operand or contract for builtin `Option`
  helpers — receiver of `unwrap_or` / `is_some` / `is_none` is not an
  `Option[T]` value, `unwrap_or` default incompatible with the payload
  type, or wrong argument arity. Mirrors the shipped-helper contract
  family.
- `GOF3154` (error): implicit coercion between `T` and `Option[T]` — a `T`
  value used where `Option[T]` is required (fix-it wraps the expression in
  `Option.Some(...)`), or an `Option[T]` value used where `T` is required
  (note-only: resolve absence explicitly via `match` or `unwrap_or`;
  postfix `?` does not apply to `Option`).

Reused existing codes, no new allocation: the parameterized-annotation
arity code (`Option[int, string]`), the duplicate-arm, wrong-arm,
non-exhaustive-match, and payload-binding-arity codes for `Option` matches,
the non-boolean-condition code (`if opt:`), the postfix-`?` operand code
(`opt?`, with an `Option`-specific note), the equality-operand code
(`Option` of a payload without equality semantics), and the
conflicting-top-level-name code (user `Option` declarations).

`spec/diagnostics.md` gains `GOF3151`–`GOF3154` when the typed-HIR pass
lands.

## Test Plan

- typed-HIR / type-layer unit tests: constructor variant validation,
  payload arity, match-arm validation for `Option` scrutinees (both arms,
  no duplicates, one payload binding, wrong-namespace arms rejected),
  coercion rejection in both directions across binding/argument/return
  positions, helper contracts, and unannotated-return inference of
  `Option[T]`.
- evaluator tests: construction, match dispatch and payload binding,
  `unwrap_or` on both variants, `is_some` / `is_none`, the equality matrix
  (`None == None`, `Some(x) == Some(x)`, `Some(x) != None`, equality
  rejected for payloads without equality semantics), and rendering.
- ownership interaction (RFC 0043): value-type scrutinee reuse after
  `match`, owned payload consumption on `match` binding, handle payload
  sharing through `Option`.
- negative fixtures with exact codes: `GOF3151`, `GOF3152`, `GOF3153`,
  `GOF3154` — following the positive/negative fixture style this repo
  already uses for `Result` diagnostics.
- propagation negative fixture: `opt?` fails with the existing postfix-`?`
  operand code and the `Option`-specific note.
- sequence-helper regression tests: `first` / `last` / `min` / `max` still
  return `Result[element, RuntimeError]` with `EmptySequence` — guarding
  the no-migration decision.
- conformance fixtures and doctests: compile-and-run programs covering the
  full surface, wired into `gof test` and `gof test --docs`.
- CLI conformance: end-to-end `gof test` runs over the new fixtures.
- VS Code extension (AGENTS.md §9): grammar/snippet coverage for the new
  syntax; `npm test` and `npm run package`.
- docs/site and spec synchronization per AGENTS.md §8, with the site build
  verified for the updated pages.

## Benchmark Impact

No dedicated bootstrap benchmark gate. `Option` reuses the enum-shaped
payload path that `Result` already exercises: construction, `match`
dispatch, and the three helpers are O(1) operations structurally identical
to existing enum operations, and the evaluator adds no allocation beyond
the payload itself. Compile-time cost is a fixed two-variant exhaustiveness
check, O(1) with no effect on incremental compile budgets.

Native-target expectations are deliberately **not** claimed by this RFC.
The candidate optimization — niche encoding of `Option.None` into the
payload's representation (zero-cost `Option` of owned and handle payloads)
— is recorded as a Phase 2 backend gate, so any future zero-cost claim
stays benchmark-backed rather than asserted, per VISION.md's honesty rules.
