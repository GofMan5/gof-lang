# RFC 0052: Full Patterns

## Summary

Complete the pattern-matching surface that VISION Phase 1 calls "полные
паттерны с guards" (full patterns with guards), extending the tuple-pattern
grammar RFC 0050 already fixed:

- **one recursive pattern grammar** for all four pattern positions — `match`
  arm heads, enum / `Result` / `Option` payload positions, binding statement
  targets, and `for` loop bindings. RFC 0050's positional tuple patterns now
  nest inside enum payloads inside tuples in every direction:
  `Result.Ok(Option.Some(x))`, `RuntimeError.HttpStatus(404, body)`,
  `(Gesture.Pan((dx, dy)), "drag")`
- **wildcard `_`** — matches anything, binds nothing, legal in every pattern
  position; fixes the "naming the ignored member" ceremony RFC 0050
  explicitly named as a cost of not having wildcards
- **literal patterns** — `int` literals (optional leading `-`), `string`
  literals, `true` / `false` — as refinement sub-patterns and as arm heads
- **guards** — `pattern if <bool expr>:` on `match` arm heads only, reusing
  the existing `if` keyword; no new reserved words
- **widened `match` scrutinee domain** — enum, `Result`, `Option` (as
  today) plus `int`, `string`, `bool`, and tuples; statement-level `match`
  stays a statement
- **exhaustiveness with literals and guards** — guards and literal
  refinement never count toward coverage; every variant (or the whole
  scrutinee type, for primitives and tuples) needs one **unguarded** arm
  that is irrefutable at every position. Every `match` dispatches at
  runtime: no fall-through hole, no new panic path
- **explicit arm-order semantics** — arms are tested top to bottom, first
  match wins; a variant may appear in multiple arms when their literal
  skeletons or guards differ; exact duplicate arms and unreachable arms
  are errors
- **ownership per RFC 0043** — literal and wildcard leaves never move
  anything (they read or discard); binding leaves over owned positions
  move per-leaf; value-type scrutinees are never consumed by matching
- **cut, defended in Alternatives**: OR-patterns and expression-position
  `match`
- diagnostics: `GOF3163`–`GOF3167` and nothing else

## Problem

The shipped `match` contract (RFC 0003, RFC 0014, RFC 0015, RFC 0049) is a
variant dispatcher: every arm is `EnumName.Variant` or
`EnumName.Variant(binding, ...)`, every payload binding is a bare name,
each variant appears exactly once, and exhaustiveness means enumerating
variants. That is a correct base but not the surface VISION Phase 1
promises, and real code already bumps into four concrete walls:

- **refinement is impossible.** The most natural dispatch questions —
  "this variant, but only with this payload value" (`HttpStatus(404, …)`,
  `NetTimeout` vs a specific message) — cannot be written. The only
  workaround is binding the payload and re-testing it with `if` inside the
  arm body, which hides the branch from the arm head, from exhaustiveness,
  and from the reader
- **literal dispatch has no home.** Branching on an `int`, `string`, or
  `bool` value requires `if` / `else` chains, and `gof` has no `elif`, so
  every additional case deepens the nesting. A `match` over a non-enum
  value is rejected outright (`GOF3032`)
- **the wildcard is already de facto.** `_` lexes as an ordinary
  identifier today, and shipped source uses it as a discard convention in
  match payload positions (`Result.Ok(_)` in
  `examples/http_status_gate.gof`, `Result.Err(_)` in
  `examples/process_capture.gof`) — a convention without a contract: it
  silently creates a binding named `_` that nothing reads. RFC 0050
  additionally cites the cost of its absence in destructuring:
  "forcing full destructuring — and naming the ignored member, since
  there are no wildcards — is ceremony"
- **RFC 0050 pinned the seam and deferred the rest.** Its out-of-scope
  list names exactly this RFC: "literal, wildcard, or guarded
  sub-patterns" and "match on a tuple scrutinee … belong to the
  full-patterns Phase 1 slice". The tuple grammar landed as
  *destructures-only*: leaves must be identifiers. Two grammar decisions
  therefore cannot be deferred further: what `_` means in pattern
  position, and which keyword a guard uses — both must be fixed before
  the pattern AST exists, because every later destructuring feature
  (struct patterns, sequence patterns) compounds whichever answers land
  here

Deciding these now also keeps one grammar instead of several: RFC 0050
already unified tuple destructuring across statements, loops, and match
payload positions. This RFC generalizes that grammar to literals,
wildcards, guards, and full recursion, so "pattern" stays one concept the
language teaches once.

## Proposed Change

### One pattern grammar, four positions

```text
pattern           := wildcard | literal | binding | variant | tuple
wildcard          := "_"
literal           := ["-"] int | string | "true" | "false"
binding           := identifier
variant           := identifier "." identifier
                   [ "(" pattern "," pattern ( "," pattern )* ")" ]
tuple             := "(" pattern "," pattern ( "," pattern )* ")"
guard             := "if" expression

match_arm         := pattern [ guard ] ":" block
bind_target       := [ "mut" ] irrefutable
for_binding       := irrefutable
irrefutable       := wildcard | binding
                   | "(" irrefutable "," irrefutable ( "," irrefutable )* ")"
```

The same `pattern` nonterminal serves all four positions with one
restriction: **binding statements and `for` bindings accept only the
irrefutable subset** (bindings, wildcards, tuples of irrefutable leaves).
A literal or enum-variant leaf there is `GOF3163` — those patterns can
fail to match, and a binding statement is not a branch. **Guards are
match-arm-only**: after a binding-statement or `for` pattern the parser
expects `=` / `in`, so `if` there is a parse error with a specific
message, not a typed-HIR check.

One binding rule holds in every position: **a pattern may not bind the
same name twice**. `(x, x)` and `Variant(x, x)` are the existing
duplicate-local-binding diagnostic (`GOF3006`) with the pattern's span —
pattern bindings are function-scope locals, so the existing contract
covers them. `_` is exempt by definition: it binds nothing.

`gof` has no comma expressions, so this grammar occupies only space that
is already invalid: today a match arm head must start with
`Identifier . Identifier`, so a literal-, `(`-led, or `_`-led arm head is
a parse error, and RFC 0050 already opened `(`-led binding statements and
`for` bindings. Tuple arity rules (two or more members, no trailing
comma, no 1-tuples) are RFC 0050's, unchanged, and `GOF3155` keeps its
contract for pattern-position arity.

### Wildcard `_`

- `_` matches any value of any position type and binds nothing
- `_` may appear any number of times in one pattern
- `_` is not a name: reading it after any pattern is the existing
  unknown-binding diagnostic (`GOF3002`) with a note that `_` discards
- `_` stays a valid identifier everywhere outside pattern positions —
  function names, fields, variables — exactly as today. Wildcard-ness is
  a property of pattern position, not a new reserved word

Compatibility rests on an audit, not an assertion: `_` currently lexes as
`Ident("_")` and payload positions bind it as an ordinary name. Repo-wide,
the only pattern-position uses are `Result.Ok(_)` in
`examples/http_status_gate.gof` and `Result.Err(_)` in
`examples/process_capture.gof`; both are write-only (the arm bodies never
read `_`), so converting them from "binds a name nothing reads" to
"discards" is observationally identical. A program that both binds `_`
and later reads it would now fail with `GOF3002` — that style is
unidiomatic noise, and the note on the diagnostic explains the change.
`_ = expr` in a binding statement evaluates `expr` for its effects and
discards the value — the explicit "call and ignore" idiom, replacing the
accidental binding it created before.

### Literal patterns

- `int` literals, with an optional leading `-` (token sequence, not an
  evaluated expression), `string` literals with the existing lexer
  escapes, and `true` / `false`
- a literal pattern matches exactly the values equal to it, using the
  position type's existing equality semantics; no new comparison rules
- `unit` has no literal pattern (one value; a binding or `_` is total),
  and no other type has literals, so nothing else participates
- **an identifier in pattern position is always a binding, never a value
  lookup.** `match mode: DEBUG:` binds the scrutinee to a fresh `DEBUG`;
  it does not compare against anything. This is the classic
  identifier-vs-constant trap (Rust inherits the same semantics and the
  same surprise); the rule is stated loudly because there is no named-
  constant pattern in v1 — see Alternatives. When a top-level binding
  arm's name happens to match a variant name of the scrutinee's enum,
  the compiler's unreachability and coverage rules almost always surface
  the mistake immediately (the intended variant arms after it become
  unreachable, `GOF3167`)

### Guards: `pattern if expr:`

An arm head may end with a guard before its `:`:

```gof text
enum HttpOutcome:
    Done(status: int, body: string)
    Failed(code: int)

fn classify(outcome: HttpOutcome) -> string:
    match outcome:
        HttpOutcome.Done(status, body) if status == 404:
            return "missing: " + body
        HttpOutcome.Done(status, body) if status >= 500:
            return "server error: " + to_string(status)
        HttpOutcome.Done(status, body):
            return "ok " + to_string(status)
        HttpOutcome.Failed(code):
            return "transport failure " + to_string(code)
```

Semantics, in evaluation order:

1. the scrutinee expression is evaluated exactly once, before any arm
2. arms are tested top to bottom; within an arm, the pattern is tested
   first
3. when the pattern matches, its bindings are created, then the guard
   (if present) is evaluated — guards see the arm's bindings plus every
   name visible at the `match` statement, and nothing else
4. a `true` guard runs the arm body and ends the `match`; a `false`
   guard falls through to the next arm; bindings are scoped to the arm
   (guard and body) and are immutable
5. a `match` with no successful arm is impossible by construction (see
   Exhaustiveness)

**Why `if`.** Three reasons, in priority order. First, it is an existing
keyword — `gof` adds no new reserved word, so no program using `when` or
`where` as an identifier breaks (AGENTS.md §1: borrow strengths, not
ancestors' token baggage; §5: surface-area growth is last in the priority
order). Second, both pattern-matching ancestors `gof` synthesizes —
Python (PEP 634 `case … if …`) and Rust — spell guards with `if`, so the
form is already familiar to both audiences. Third, the position is
unambiguous: patterns cannot contain `if`, no expression form starts a
block, and expression parsing is bracket-aware, so the guard expression
extends to the arm's terminating `:` without grammar conflicts (a `:`
inside a dict literal in a guard is consumed by dict parsing, as in every
other expression position). `when` (C#/Kotlin/Scala) and a bare-comma
predicate were rejected — see Alternatives.

**Guard expression contract** (`GOF3165` for violations):

- the guard must resolve to `bool` — no truthiness, no coercion, same
  discipline as `if` and `while` conditions
- the guard may not contain postfix `?`, `await`, or `go` expressions. A
  guard is a predicate over values; error propagation, blocking task
  joins, and spawning are control flow, and hiding them inside a
  predicate makes the arm's behavior un-local. Unwrap or join before the
  `match`, or move the work into the arm body
- ordinary calls are allowed (`len(x) > 0`, `contains(parts, "a")`); if a
  callee blocks, that blocking is the callee's existing contract, visible
  at the call site — the guard adds nothing new
- side effects in guards run at a documented point — after the arm's
  pattern matched, before its body, only for that arm — and are
  deterministic in arm order

Guards are legal on any arm, including total ones (`_`, a binding, or an
irrefutable tuple arm); a guarded total arm no longer counts as total —
which is the point of the exhaustiveness rule below.

### Nested destructuring — full recursion

Payload positions and tuple leaves accept the full pattern grammar,
including each other:

- **tuple in a payload** (RFC 0050): `Gesture.Pan((dx, dy))`,
  `Result.Ok((q, r))`
- **enum in a payload**: `Result.Ok(Option.Some(value))` for
  `Result[Option[T], E]` scrutinees; unit variants like `Option.None`
  work as sub-patterns
- **literal in a payload**: `RuntimeError.HttpStatus(404, body)` — match
  one status code and bind the body; `HttpOutcome.Done(status, 0)` —
  assert a payload value while binding the rest
- **enum in a tuple leaf, tuple scrutinee arm heads** — a `match` whose
  scrutinee is a tuple expression dispatches on both members:

```gof text
enum Gesture:
    Pan(delta: (int, int))
    Pinch(start: (int, int), end: (int, int))

fn route(event: (Gesture, string)) -> string:
    match event:
        (Gesture.Pan((dx, dy)), "drag") if dx == 0 and dy == 0:
            return "tap"
        (Gesture.Pan((dx, dy)), "drag"):
            return "pan " + to_string(dx) + "," + to_string(dy)
        (Gesture.Pinch((x0, y0), (x1, y1)), tag):
            return "pinch " + tag
        (_, tag):
            return "unrouted " + tag
```

Construction and destructuring stay mirror images, per RFC 0050: the arm
`(Gesture.Pan((dx, dy)), "drag")` destructures exactly the values the
expression `(Gesture.Pan((dx, dy)), "drag")` constructs. Pattern
positions carry no type annotations — member types are statically known
from the matched value, as in RFC 0050.

### Patterns in binding statements and `for` bindings

RFC 0050's grammar gains wildcard leaves; literals and variants stay
excluded:

```gof text
point = (3, 4)
(_, y) = point
mut (w, _) = (10, 2)
for (i, _) in [(0, "a"), (1, "b")]:
    print(i)
```

- leaves follow RFC 0050's per-leaf rules unchanged: a fresh name creates
  an immutable (or `mut`-declared) local; an existing `mut` name is
  reassigned; an existing immutable name keeps today's immutable-
  reassignment error. `_` leaves participate in none of that — they bind
  nothing
- the right-hand side is fully evaluated before any target is written
  (RFC 0050's swap rule, unchanged); an all-wildcard target evaluates the
  right-hand side for its effects and discards
- `mut` on a pattern with no binding leaves is accepted and has no
  effect (future lint, not an error)
- a literal or variant leaf is `GOF3163`: this pattern can fail to match,
  and a binding statement is not a branch — use `match`

### `match` scrutinee domain

`match` accepts: enums, `Result[T, E]`, `Option[T]`, `int`, `string`,
`bool`, and tuples (recursively composed of those). Arm patterns must
belong to the scrutinee's domain — a variant arm must name a variant of
the scrutinee's enum (`GOF3031`, existing), a literal arm must have the
scrutinee's literal type (`GOF3164`), a tuple arm must match arity
(`GOF3155`, existing).

Not accepted: `unit` (one value — nothing to dispatch), `list`, `dict`,
`json` (no sequence patterns in v1), structs (no field patterns in v1),
and handle types including `channel`, `task`, and `cancel_token` — those
keep `GOF3032`, whose note now lists the accepted scrutinee kinds. See
Alternatives for why struct and sequence patterns are out of scope.

### Exhaustiveness — guards and literals never count

One rule, uniform across every scrutinee kind and every pattern position:

> Erase guards, then require that every variant of an enum scrutinee —
> or the whole value space of a primitive or tuple scrutinee — is covered
> by an arm that is **irrefutable at every position**.

"Irrefutable at a position" means the pattern there is a binding, `_`, or
a tuple of irrefutable leaves. Concretely:

- **enum / `Result` / `Option`**: variant `V` is covered iff some
  **unguarded** arm for `V` has only irrefutable payload sub-patterns.
  `HttpStatus(404, body)` alone does not cover `HttpStatus` — it refines
  it; a second unguarded `HttpStatus(code, body)` (or a `_` arm) does
- **`int` / `string`**: literal arms never cover — the domains are
  infinite — so an irrefutable arm (`_` or a binding) is always required
- **`bool`**: same rule; `true:` + `false:` do not cover in v1 (see
  below), so an irrefutable arm is required
- **tuple**: covered iff some unguarded arm has all-irrefutable leaves.
  In the `route` example above, the final `(_, tag)` arm is required:
  arm 2's first leaf is the variant pattern `Gesture.Pan(...)`, which
  covers only that variant

Non-coverage is the existing non-exhaustive diagnostic (`GOF3033`),
extended to the new scrutinee kinds, with a note naming the uncovered
variant or position and suggesting the total arm.

**Why guards never count — the defense.**

1. **Soundness.** A guard is an arbitrary boolean expression; the
   compiler cannot bound its truth (`f(x)`, side effects, arbitrary
   predicates — this is halting-problem territory). Treating a guarded
   arm as covering would open a runtime state where no arm runs. A
   statement-level `match` in that state must either panic or silently
   do nothing. Panic violates the language contract — `panic` is for
   broken invariants, and "a value I did not enumerate" is a routine
   input, not a broken invariant. Silent fall-through is worse: the
   `match` statement claims to be total and then isn't. Erasing guards
   before the check is what keeps the RFC 0003 promise — exhaustive
   `match` — true by construction: every value reaches an unguarded,
   irrefutable-covering arm, so dispatch always succeeds and generated
   code needs no hidden failure path at all
2. **Ancestor consensus.** Rust, Swift (`where`), and Python (PEP 634)
   all exclude guards from exhaustiveness. `gof` is not inventing
   semantics here; it is refusing the one relaxation every ancestor
   already rejected
3. **One sentence to teach.** "Literal and guarded arms refine; an
   unguarded irrefutable arm per variant is what makes the `match`
   total." The cost is one explicit fallthrough arm — which is exactly
   where "and everything else means X" belongs, and which keeps
   compiling correctly when the scrutinee's type later grows: adding a
   variant breaks the match at the site that must decide, which is the
   strict-exhaustiveness contract gof has had since RFC 0003
4. **Runtime freedom.** Because totality never depends on guard truth,
   the compiler may reorder and merge the unguarded covering tests
   freely; guarded arms are pure refinement evaluated only after their
   pattern matched

The `bool` case is the honest cost of uniformity: `true:` + `false:`
provably covers `bool`, and a full usefulness checker (Rust/Swift-style)
would accept it. v1 deliberately ships the conservative rule — one rule
for every position, no product-space analysis. The general check
(including literal-set coverage and subsumption) is recorded as future
work with its soundness notes; until then the one-line total arm closes
every `bool` match.

### Duplicate and unreachable arms

Arm order is now significant (repeated variants, guards, literals), so
the arm set needs explicit shape rules:

- **repeated variants are legal** when the arms differ — different
  literal skeletons, or a guard. This is the refinement idiom this RFC
  exists for. This narrows the shipped "each variant at most once" rule:
  `GOF3030` now fires only for **exact duplicate arms** — two unguarded
  arms whose skeletons are equal (same variant, same literal values in
  corresponding positions, bindings and wildcards interchangeable as
  slots). On code that compiles today the behavior is identical: today's
  repeated-variant arms are always unguarded slot-skeleton duplicates.
  The refinement idiom — a guarded arm followed by an unguarded arm
  with the same skeleton — stays legal by definition, because the
  duplicate check considers only unguarded arms
- **duplicate detection on widened scrutinees**: the same skeleton rule
  over `int` / `string` / `bool` / `tuple` scrutinees is `GOF3166`
  (`404:` twice; `(0, x):` then `(0, _):`). The code family follows the
  scrutinee: enum-family duplicates keep `GOF3030`, primitive/tuple
  duplicates are `GOF3166` — deterministic boundary, no overlap
- **unreachable arms are errors** (`GOF3167`), preserving gof's
  dead-code discipline and the add-variant-breaks-matches contract:
  - any arm that follows an arm irrefutable over the whole scrutinee
    (`_`, a bare binding, or — on a tuple scrutinee — an all-irrefutable
    tuple arm) can never run
  - an arm irrefutable over the whole scrutinee is itself unreachable
    when the preceding unguarded arms already cover the match — the
    `all-variants + _` pattern is dead by construction, and allowing it
    would let a wildcard silently defeat strict exhaustiveness the day
    someone adds a variant
- **subsumption beyond these two shapes is not diagnosed in v1**: an arm
  made dead by partial overlap (`HttpStatus(404, _)` before
  `HttpStatus(404, body)`) compiles and is simply never reached — first
  match wins. Full subsumption analysis needs the usefulness check
  deferred above; shipping half of it as errors would be guesswork. This
  is recorded as future lint work, not silent scope creep

### Ownership under RFC 0043

Patterns add no new taxon and no new move site; they refine the existing
ones per-leaf:

- **value-type scrutinees are never consumed.** RFC 0043 already
  classifies `match` scrutinee evaluation as a borrow; payload bindings
  of value types duplicate under CoW. Literal and wildcard leaves
  strengthen this: they bind nothing, so matching
  `Result[Box[T], E]` with `Result.Ok(_)` is a pure read — the scrutinee
  stays usable. Only extraction moves
- **binding leaves follow RFC 0043 exactly**: a binding leaf over an
  owned position moves the value out (later use of the scrutinee is the
  existing `GOF3129`, fix-it `clone(...)`); over a value position it
  duplicates; over a handle position it shares. A tuple pattern over an
  owned-member tuple therefore consumes only through its binding leaves
  — `(_, b)` on an owned pair reads member 0 and moves member 1
- **literal and wildcard leaves never move anything**: a literal leaf is
  an equality test (RFC 0043's borrow classification for comparison
  operators); a wildcard leaf is a discard
- **guards read, never consume** — guard expressions are operators and
  calls over the arm's bindings, which RFC 0043 already classifies as
  borrows; the `?` / `await` / `go` ban keeps control flow out of the
  predicate
- **no allocation is introduced**: patterns are compile-time structure;
  the only values created are the bound ones, under the rules above

### AGENTS.md §5 guardrails answered

- **who owns the data** — the scrutinee's owner is unchanged; ownership
  crosses a pattern boundary only through binding leaves over owned
  positions, at the exact sites RFC 0043 already lists (match payload
  moves)
- **where values live** — pattern bindings are ordinary function-local
  bindings, same storage and scoping as today's match payload bindings
- **where allocation happens** — nowhere new; duplication follows the
  member taxa per RFC 0043, CoW for value members
- **where blocking can happen** — only inside guard callee calls that
  already block by their own contract; `await` in guards is rejected
  (`GOF3165`), so no join can hide inside a predicate
- **how races are prevented** — no new shared state exists; bindings are
  arm-local immutables; guards are reads
- **how the feature is debugged** — five new deterministic diagnostics
  (`GOF3163`–`GOF3167`) plus extended notes on `GOF3002`, `GOF3030`,
  `GOF3031`, `GOF3032`, `GOF3033`, and `GOF3071`, all under the
  diagnostics contract (code / severity / span / message / note /
  fix-it); runtime behavior is a documented deterministic arm order,
  pinned by conformance fixtures

### Explicitly out of scope

- OR-patterns — cut, defended in Alternatives
- expression-position `match` — stays statement-level, defended in
  Alternatives
- struct field patterns (`Point(x: 0, y: _)`) and sequence patterns
  (list / dict / `json`) — different grammars (named fields, open-ended
  shapes) that need their own RFCs; struct destructuring is served today
  by field access plus the new guards
- rest/splat patterns, variadic anything (RFC 0050 already rejected the
  variadic direction for tuples)
- named-constant or enum-value-expression patterns (`MAX_RETRIES`,
  `Mode.DEBUG` as a compared value) — see Alternatives
- range patterns (`1..5`) — gof has no range literal type; ranges are
  `list[int]` today. Guard comparisons (`if n >= 1 and n <= 5`) express
  the same intent without a new pattern kind
- subsumption/usefulness lint beyond the two unreachable-arm shapes
  specified above
- `select` arm patterns — RFC 0050 excluded them; unchanged

### Implementation mapping

- **lexer**: unchanged — no new tokens. `_` arrives as `Ident("_")` and
  is interpreted by position; guards reuse `if`; negative int patterns
  reuse `Minus` + `Int`; literals already lex
- **CST / AST**: one recursive `Pattern` node (literal, wildcard,
  binding, tuple, variant-with-payload-patterns) shared by all four
  positions — RFC 0050's tuple patterns are the same node at tuple
  positions. `MatchArm` gains `guard: Option<Expr>`; `Stmt::Bind` and
  `Stmt::For` take patterns (RFC 0050 already opened those). The parser
  accepts `(`-, literal-, and `_`-led arm heads and optional `if` before
  the arm's `:`, checks only well-formedness and leaf shape, and emits
  `GOF2001`/`GOF2002` with specific messages for guards outside match
  arms and malformed pattern leaves (RFC 0050 precedent). No
  parser-level typing (AGENTS.md §4)
- **HIR / typed HIR**: pattern typing against position types
  (`GOF3164`), the irrefutability check for binding statements and `for`
  (`GOF3163`), the guard contract (`GOF3165`), the skeleton duplicate
  check (`GOF3030` narrowed, `GOF3166`), the two unreachable-arm shapes
  (`GOF3167`), and the coverage check (guard erasure + per-variant /
  whole-type irrefutable coverage, `GOF3033`). Move tracking reuses the
  RFC 0043 checker, driven per binding leaf
- **MIR / backend (Phase 2)**: arm dispatch lowers to compare-and-
  branch chains — tag tests for variants, value comparisons for
  literals, one branch per guard; nested destructuring lowers to the
  same direct member loads RFC 0050 specified. No new IR construct
- **evaluator (bootstrap)**: ordered arm dispatch in the interpreter —
  pattern test, then guard, in arm order; no new value kinds; renders
  nothing new

## Alternatives Considered

### `when` guards instead of `if`

Rejected. `when` requires a new reserved word: every program using
`when` as an identifier breaks, and `gof` has so far reserved words only
for statements that need them. `when` buys no additional power over `if`
— the guard's position (between pattern and `:`) carries all the
meaning. C#, Kotlin, and Scala chose `when` because their `if` is an
expression or their `case` syntax already ended in a colon-heavy shape;
neither applies to gof. Reusing `if` also matches both direct ancestors:
Python's PEP 634 and Rust.

### OR-patterns — cut

Rejected for v1, deliberately and with re-entry criteria. The case for
`A | B` is real (arm deduplication), but every part of gof's design
pushes back:

- **binding consistency is a hidden type system.** `A(x) | B(y)` binds
  different names per alternative. Python resolves this dynamically (the
  unmatched name is simply unbound — dynamic chaos, the exact ancestor
  defect gof refuses). Rust requires identical bindings across
  alternatives plus a dedicated consistency check and diagnostic. gof
  would have to adopt the Rust rule, its checker, and its errors — a
  whole sub-spec for one ergonomic
- **guards interact badly.** `A(x) | B(y) if g:` — which bindings does
  `g` see? Only the same-bindings constraint answers it, and then the
  guard runs per-alternative or post-union — another semantic decision
  that must be specified, implemented, and taught
- **the token does not exist.** gof has no `|` operator and no `Pipe`
  token. OR-patterns would introduce a pattern-only token before bitwise
  operations are even designed, and a future bitwise `|` would then
  carry pattern-versus-expression overloading from day one
- **the duplicate-arm rule would need overlap analysis.** "Each variant
  at most once" would become "each variant at most once, unless inside
  an OR, unless the alternatives are distinct…" — precisely the
  complexity class this RFC's arm rules exist to avoid
- **statement-level match weakens the payoff.** The big OR win in Rust
  is compact value-producing arms. In statement match, the equivalent is
  two short bodies or one guard — and the guard already covers the most
  common multi-value refinement: `Status.Busy(n) if n == 0 or n == 10:`
  reads fine

Re-entry criteria, recorded honestly: a future RFC may add OR-patterns
once bitwise `|` exists or a dedicated separator is deliberately chosen,
the binding-consistency rule is designed, and guard composition is
specified — with usage evidence that guard-based workarounds are
insufficient. It must be its own RFC; it does not ride in on this one.

### Expression-position `match`

Rejected — `match` stays a statement. Changing it would be the largest
single semantic change in this RFC's orbit, and nothing here needs it:

- gof has **no expression-position control flow at all**: `if`, `while`,
  `for`, `select` are statements; there is no ternary. Making `match`
  the one expression-form control flow would be an anomaly — familiarity
  cosplay toward Rust rather than coherent design (AGENTS.md §1)
- an expression match needs arm-**value** typing: unifying arm result
  types, interaction with return-type inference, `?` inside arm values —
  a whole typing subsystem, orthogonal to patterns
- statement match plus early `return` already expresses the value-
  producing use case in the code style gof programs use everywhere, and
  Python — the readability ancestor — reached the same conclusion:
  match is a statement there too

Re-entry: a future RFC could propose expression `match` (probably
together with expression `if`), against measured usage pain. Until then,
patterns land in the statement position the language already fixed.

### Full usefulness / literal-set coverage analysis

Deferred, honestly. A Rust/Swift-style usefulness checker would accept
`true:` + `false:` over `bool`, literal sets that provably cover finite
domains, and would diagnose every subsumed arm instead of two shapes.
It is a real algorithm (pattern matrices, product-space coverage —
per-position unions are not sound: arms `(true, false)` and
`(false, true)` project to "both positions total" yet cover only two of
four values). v1 ships the conservative rule — one sentence, one small
checker, no soundness traps — and records the fuller check as future
work with its soundness requirement stated, so nobody "simplifies" it
into the unsound version later.

### Named-constant patterns

Rejected for v1. Allowing identifiers to compare against in-scope
constants (Rust's const patterns) creates the identifier-means-binding-
or-value ambiguity in every pattern position, plus resolution rules for
which identifiers are constants. The v1 rule — identifiers in patterns
always bind — is one sentence and statically unambiguous. Constants-in-
patterns can arrive later behind an explicit sigil (`const.NAME`) if
usage pain appears; the trap is documented in this RFC instead of
half-solved.

### Struct field patterns and sequence patterns

Rejected for v1. Struct patterns need a named-field grammar
(`Point(x: 0, y: _)`) — a second positional/named axis to specify, for a
type whose total shape is already destructurable by field access plus
the new guards. Sequence patterns (list length + element patterns)
introduce open-ended, variable-length matching — the variadic machinery
RFC 0050 rejected for tuples, in a new position. Both are coherent
future RFCs on top of this grammar; neither blocks Phase 1, and neither
is snuck in half-designed here.

### Guards in binding statements (`(x, y) if x > 0 = pair`)

Rejected. A refutable binding with a guard is `if let` — a conditional
branch with an implicit else hole. gof's binding statements are total
(irrefutable targets only, `GOF3163`), and the conditional-destructuring
use case is exactly `match` with a total arm. Adding `if let` would
create a second, weaker match form with a fall-through-it-doesn't-have
problem; the honest shape is the one statement that already guarantees
dispatch.

### Bare-comma guard (`Variant(x), x > 10:`)

Rejected. It saves four characters and costs a bare comma form that
exists nowhere in gof's grammar (the same argument RFC 0050 used to
reject bare comma targets), plus ambiguity with future payload
separators. `if` is one token, greppable, and already reserved.

## Compatibility Impact

### Implementation phasing amendment (review-adjudicated)

This RFC lands in two phases. The phases are a scoping decision recorded
after implementation review, not a silent cut:

- **Phase A (shipped)**: payload-position refinements — literal, wildcard,
  and tuple sub-patterns inside `Enum.Variant(...)` payload positions
  (including nested tuple leaves), `_` in binding statements and `for`
  bindings, `_ = expr` discard, guards on variant-led arms with the full
  GOF3165 contract, guard-erasure exhaustiveness, the skeleton duplicate
  rule, and the moved GOF3163 irrefutability check (typed-layer rejection
  of refutable bind/for leaves). The scrutinee domain stays
  enum / `Result` / `Option`.
- **Phase B (deferred, with re-entry criteria)**: the widened top-level
  scrutinee domain (`int`, `string`, `bool`, tuple scrutinees with
  literal/binding/wildcard-led arm heads and the `route(event: (Gesture,
  string))` tuple-dispatch shape), and enum-variant sub-pattern recursion
  inside payloads (`Result.Ok(Option.Some(value))`). Re-entry: when the
  pattern subsystem gains a second consumer that needs primitive dispatch
  (Phase 2 codegen lowering, or a stdlib API returning primitive-tagged
  unions), or when a user-facing complaint demonstrates the gap. Until
  then GOF3032's note honestly names the accepted domain
  (enum / `Result` / `Option`).

Two boundary decisions land with Phase A and are normative for it:

- **GOF3166 boundary**: exact unguarded skeleton duplicates fire GOF3030
  when the skeleton is all slots (`V(_)` and `V(x)` are equal) and GOF3166
  when the skeleton contains at least one literal. This diverges from the
  Diagnostics Impact table below, which assigned GOF3166 only to primitive
  and tuple scrutinees — that assignment presupposed Phase B's domain and
  is superseded by this amendment for the enum domain.
- **Duplicate leaves in BIND statements** follow RFC 0050's per-leaf
  reassign semantics (an immutable duplicate leaf is GOF3003; a `mut`
  duplicate leaf reassigns, last write wins), not the blanket GOF3006
  rule stated for match payloads and loop leaves. RFC 0050 owns bind
  semantics; RFC 0052's blanket wording applied to payload/for positions
  only.

The GOF3165 fix-it ("use a comparison ...") is additive beyond the
"no fix-it" wording in the Diagnostics Impact table and is retained.

### Original compatibility analysis

- **existing programs keep compiling, with one bounded exception.**
  Every new syntactic form — literal, wildcard, tuple-scrutinee, and
  guarded arm heads, `_` leaves in binding statements and `for`
  bindings — occupies space that is a parse error today. The exception:
  `_` in pattern positions stops binding a name named `_`. The repo-wide
  audit found two such usages (`Result.Ok(_)` in
  `examples/http_status_gate.gof`, `Result.Err(_)` in
  `examples/process_capture.gof`), both write-only, so behavior is
  observationally identical; only a program that reads `_` after
  binding it would fail (`GOF3002`, with an explanatory note)
- **`GOF3030` trigger narrows** from "the same variant appears twice" to
  "two unguarded arms share an exact skeleton". On every program that
  compiles today these coincide (today's arms are unguarded and their
  payload patterns are all slots), so shipped behavior is unchanged;
  previously-rejected refinement matches now compile — the additive
  direction only
- **arm-order semantics become normative** where they were previously
  free (each variant once + full coverage made order irrelevant). The
  evaluator already dispatches in arm order; this RFC pins that as the
  contract
- **no new tokens and no new reserved words** — `if` is reused, `_` is
  contextual in pattern positions, negative literals reuse `Minus`
- **ownership contracts untouched** — patterns add no taxon and no new
  move site; per-leaf behavior follows RFC 0043
- **public surfaces that must update with the implementation**:
  `spec/language-v1.md` (match contract lines: arm forms, the
  "each variant at most once" rule becomes the skeleton rule, scrutinee
  domain, binding and `for` pattern leaves, guard rules),
  `spec/diagnostics.md` (five new codes; updated wording for `GOF3030`,
  `GOF3031`, `GOF3032`, `GOF3033`, `GOF3071`), `docs/site/`, `examples/`
  (a full-patterns walkthrough), `roadmap.md` (Phase 1 full-patterns
  item), and `tools/vscode-gof` grammar, snippets, and docs for arm
  heads, guards, and wildcards
- RFC 0050 landed before this RFC (commits 55731e1..a3fa325); the two
  share one pattern grammar (BindingPattern grown by this RFC) and the
  shared arm-shape machinery in ast.rs, rather than two stacked parsers

## Diagnostics Impact

The last reserved code is `GOF3157`; `GOF3158`–`GOF3162` stay reserved
for concurrent in-flight sibling RFCs and are not touched. This RFC
allocates `GOF3163`–`GOF3167` and nothing else. All five follow the
diagnostics contract (code / severity / span / message / note / optional
fix-it):

- `GOF3163` (error): **refutable pattern in an irrefutable position** —
  a literal, enum-variant, or partially-literal tuple pattern used as a
  binding-statement target or `for` loop binding. Note: "this pattern
  can fail to match; use a `match` statement to handle the failing
  case". No fix-it — the resolution depends on intent
- `GOF3164` (error): **pattern incompatible with its matched position's
  type** — a literal pattern whose type differs from the position type
  (`match name: 42:` on a `string`; `Done("x")` on an `int` payload), a
  variant sub-pattern against a non-enum position, or a variant arm
  naming an enum type other than the scrutinee's. Note names the
  expected and found types and positions. Boundary with RFC 0050:
  `GOF3156` keeps every contract RFC 0050 gave it (tuple member types on
  the expression and type side, tuple patterns applied to non-tuple
  payload fields); when a **literal** mismatches, even inside a tuple
  leaf, the code is `GOF3164` — the literal is the new pattern kind, so
  the rule stays deterministic
- `GOF3165` (error): **invalid `match` guard contract** — a guard
  expression that does not resolve to `bool`, or a guard containing
  postfix `?`, `await`, or `go`. Notes: for truthiness — "guards must
  resolve to `bool`"; for control flow — "unwrap, join, or spawn before
  the `match` or inside the arm body". No fix-it
- `GOF3166` (error): **duplicate `match` arm pattern** — two unguarded
  arms with the same skeleton over an `int`, `string`, `bool`, or tuple
  scrutinee (`404:` twice; `(0, x):` then `(0, _):`). Fix-it: remove the
  duplicate arm. Enum-family duplicates keep `GOF3030`
- `GOF3167` (error): **unreachable `match` arm** — an arm that follows
  an arm irrefutable over the whole scrutinee, or such an arm when the
  preceding unguarded arms already cover the match. Note names the
  arm that shadows it or the coverage that precedes it. Fix-it: remove
  the unreachable arm

Reused existing codes, no new allocation: `GOF3002` (reading `_`, with
an explanatory note), `GOF3006` (a pattern that binds the same name
twice — pattern bindings are function-scope locals), `GOF3030` (enum-
family exact duplicates, trigger narrowed as specified), `GOF3031`
(wrong-variant arms, now also in sub-pattern positions), `GOF3032`
(scrutinee without a pattern domain, note now lists the accepted kinds),
`GOF3033` (non-exhaustive, extended to the new scrutinee kinds and the
guard-erasure rule), `GOF3071` (payload pattern count vs declared
payload arity), `GOF3129` (use of a scrutinee consumed by an owned-
payload binding leaf), `GOF3155` (tuple pattern arity, RFC 0050's
contract), and `GOF2001`/`GOF2002` (guards outside match arms and
malformed pattern leaves — parser-level, with specific messages, per the
RFC 0050 precedent). `spec/diagnostics.md` gains the five new codes when
the typed-HIR pass lands.

## Test Plan

- **parser / AST unit tests**: arm-head forms (identifier, `_`, literal
  including `-` and strings with escapes, `(`-led tuple, variant with
  and without payload patterns); guard parse and its termination at the
  arm `:`; guard rejection in binding statements and `for` (specific
  `GOF2001`/`GOF2002` messages); nesting in every direction
  (`Result.Ok(Option.Some(x))`, `(Gesture.Pan((dx, dy)), "drag")`);
  RFC 0050 degenerate shapes still `GOF3155`; wildcard leaves in binding
  statements and `for` bindings
- **typed-HIR tests**: the `GOF3164` matrix (literal vs position type
  across head, payload, and tuple-leaf positions; variant vs non-enum
  position); `GOF3163` for every refutable leaf in binding and `for`
  positions; `GOF3165` (non-bool guard, `?`/`await`/`go` in guards);
  `GOF3166` and narrowed `GOF3030` (skeleton duplicates, refinement-
  idiom non-duplicates); `GOF3167` both shapes (arm after a total arm;
  total arm after complete coverage, including the bool case); `GOF3033`
  coverage matrix (enum variant with literal-only refinement; int and
  string literal-only matches; bool without a total arm; tuple without
  an all-irrefutable arm; guards erased — a match exhaustive only
  through a guarded arm fails); `GOF3006` duplicate binding names in one
  pattern; `GOF3002` reading `_`
- **evaluator and conformance**: dispatch order (repeated variants with
  literals and guards — first match wins); guard evaluation timing (side
  effects observed only for the matched arm, pattern-before-guard
  ordering); scrutinee evaluated exactly once; binding visibility in
  guards and bodies; wildcard discard; literal equality for negative
  ints and strings; nested destructuring values end-to-end; `for` and
  binding-statement wildcard leaves; snapshot output for the
  full-patterns example
- **RFC 0043 boundary tests**: value-type scrutinee reusable after every
  new pattern kind; `Result[Box[T], E]` matched with `Result.Ok(_)`
  stays usable (non-consuming read) while `Result.Ok(b)` consumes
  (`GOF3129` on later use); owned-member tuple with a wildcard leaf
  consumes only through binding leaves
- **fixtures**: positive and negative compile fixtures for each of
  `GOF3163`–`GOF3167`, plus extended-`GOF3033` and narrowed-`GOF3030`
  fixtures, following the repo's existing fixture discovery surfaces
- **regression**: the two audited `_` payload bindings
  (`examples/http_status_gate.gof`, `examples/process_capture.gof`)
  compile and run with identical output
- **docs and examples**: `examples/full_patterns.gof` walkthrough; spec
  and `docs/site` sync per AGENTS.md §8; doctests for the guard syntax,
  the wildcard rule, and the coverage rule
- **VS Code extension (AGENTS.md §9)**: grammar and snippets for guarded
  arms, literal and wildcard arm heads, and pattern leaves in bindings
  and loops; `npm test` and `npm run package` verified

## Benchmark Impact

- **no new runtime construct**: patterns are compile-time structure.
  Dispatch lowers to compare-and-branch chains — tag tests, literal
  comparisons, one branch per guard — and nested destructuring lowers to
  the direct member loads RFC 0050 already specified. No allocation, no
  vtable, no new value header
- **compile time**: the coverage, duplicate, and unreachability checks
  are linear in total pattern nodes per `match` (guard erasure is
  deletion; irrefutable coverage is a per-arm fold); no symbolic
  execution, no matrices. No measurable effect on incremental compile
  budgets is expected, and none is claimed without a number
- **Phase 2 native gates**: a literal-refined `match` over `int` matches
  the equivalent hand-written `if` / `else` chain in generated code;
  guarded arms add exactly one branch per guard; deeply nested patterns
  must not reintroduce per-test allocation. These are recorded gates to
  measure, not assertions — per VISION honesty rules, no zero-cost
  claim is accepted without a recorded number
- **bootstrap evaluator**: ordered arm dispatch adds one interpreter
  loop per `match` statement, bounded by arm count; no dedicated
  benchmark gate beyond the existing conformance runtime profile
