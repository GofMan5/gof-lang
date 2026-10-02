# RFC 0050: Tuples

## Summary

Add fixed-arity anonymous tuples to `gof`, as the value-type composite that
RFC 0043 already classifies:

- type syntax `(T1, T2, ...)`, **two or more members**, structural type
  identity, positional, nestable
- literal syntax `(expr, expr, ...)`, same arity rule; **the comma
  decides** — `(expr)` stays parenthesized grouping, so there are no
  1-tuples and no 0-tuples; `(x,)`, `()`, and `(T,)` are compile errors
- one tuple-pattern grammar for destructuring, reused in binding
  statements (`(a, b) = pair`, `mut (a, b) = pair`), loop bindings
  (`for (a, b) in pairs:`), and `match` payload positions
  (`EnumName.Variant((a, b), c)`, `Result.Ok((code, body))`) — positional
  bare bindings with exact arity, the existing match payload binding style
  generalized to nest
- positional element access `pair.0` with compile-time-checked literal
  indices
- tuples flow as first-class values: function parameters and returns,
  `list` / `dict` / `channel` / `task` / `Result` type arguments, struct
  fields, enum payload fields, fixture value types, container elements
- ownership per RFC 0043: a tuple is a composite of its members — value
  members duplicate under CoW, handle members share the underlying
  resource, and a tuple containing an owned member moves as a unit
- equality and lexicographic ordering hold exactly when every member
  supports the operation
- no variadic tuples, no arity-generic tuples, no splats, no tuple
  iteration — fixed arity is the contract

This RFC delivers the tuples item from the VISION Phase 0 core RFC set and
the Phase 1 semantic core. It does not claim full literal patterns,
wildcards, guards, or record types.

## Problem

`gof` has exactly one anonymous composite today: the homogeneous `list[T]`.
Every heterogeneous fixed-size shape has to become a named `struct`:

- multi-return values are the most common fixed shape in real code —
  min/max, status + body, quotient + remainder, key + value. Forcing a
  one-off struct per shape is naming ceremony for values that have no
  domain identity of their own
- RFC 0043 already commits to tuples as one of its composite taxa
  ("enums/structs/tuples/`Result` composed of value fields"), and VISION
  Phase 1 lists tuples in the semantic core — the ownership model refers
  to a type the language cannot currently write
- two grammar decisions cannot be deferred. `(expr)` is expression
  grouping today (the parenthesized branch of `parse_primary` in
  `compiler/gof-compiler/src/ast.rs`), so a tuple literal grammar must
  resolve the paren collision explicitly or silently break grouping. And
  destructuring must pick one binding syntax now: a second positional
  binding grammar added later would collide with whatever lands first
- `gof` has no comma expressions, which is what makes both decisions
  tractable: commas today only separate list items, dict entries, call and
  constructor arguments, and match payload bindings

## Proposed Change

### The paren collision: the comma decides

A tuple is parentheses around **two or more** comma-separated items. The
parentheses alone never create a tuple — the comma does:

| Form | Meaning |
|---|---|
| `(x)` | parenthesized grouping — unchanged |
| `((x, y))` | grouping around a tuple literal — the tuple `(x, y)` |
| `(x, y)` | 2-tuple literal, 2-tuple type, or 2-tuple pattern (by position) |
| `(x,)` | error `GOF3155` — no single-element tuples |
| `()` | error `GOF3155` — no empty tuple; the unit type is `unit` |
| `f(x, y)` | call with two arguments — commas at call level stay argument separators |
| `f((x, y))` | call with one tuple argument |
| `(T)` | error `GOF3155` in type position — the type grammar has no grouping; write `T` |

The rule holds in expression, type, and pattern positions with one
explicit asymmetry: a single item inside parentheses is grouping in
**expression** position, where grouping serves operator precedence, and an
error in **type and pattern** positions, where there is no precedence to
control and the parentheses can only signal an attempted 1-tuple. So
`Result.Ok((a))` is `GOF3155` with a fix-it to drop the parentheses, while
`Result.Ok(a)` binds the whole payload.

This resolves the collision compatibly in both directions:

- `(x)` must stay grouping. If `(x)` became a 1-tuple, every redundant
  parenthesis in existing code would silently change meaning — `f((x))`
  would go from a one-argument call to a tuple argument. Banning 1-tuples
  is therefore a compatibility requirement, not only a design preference
- `(a, b)` cannot break anything today: with no comma expressions, the
  parser currently rejects `(a, b)` with "expected `)` to close grouped
  expression" at the comma, and parenthesized types were likewise parse
  errors. Tuple syntax only occupies space that was already invalid

Tuples reject trailing commas, exactly as list and dict literals do today
(the parser requires an expression after every comma). `(x,)` is therefore
always the 1-tuple attempt and gets the specific `GOF3155` diagnostic with
a remove-the-comma fix-it rather than a generic parse error.

### Type syntax and identity

`(T1, T2, ...)` with two or more members is a type, usable wherever types
are usable today: parameter and return annotations, local binding
annotations, type arguments of parameterized builtins, struct fields, and
enum payload fields. It nests: `((int, int), string)`.

Type identity is structural and positional: two tuple types are the same
type if and only if they have the same arity and pairwise identical member
types. `(int, string)` and `(string, int)` are different types.

```text
pattern   := identifier
           | "(" pattern "," pattern ( "," pattern )* ")"
tuple     := "(" expr "," expr ( "," expr )* ")"
tupletype := "(" type "," type ( "," type )* ")"
```

No trailing comma in any of the three forms. There is no hard arity cap;
tuples are intended for two to four members, and larger fixed records
should be structs. Enforcing that taste is future lint work, not an error.

### Ownership: tuples under RFC 0043

Tuples add no new taxon. They are RFC 0043 composites, and their
duplication behavior follows their members:

- **value members.** A tuple whose members are all value types is a value
  type: `Copy` with copy-on-write. Binding, argument passing, container
  insertion, struct and enum construction, `match` payload binding, and
  destructuring duplicate it; duplication is O(1) per member (reference
  bump in the native target) and reads never consume. `(1, "x")` behaves
  exactly as `int` and `string` do today
- **handle members.** Handle-typed members are allowed. Duplicating the
  tuple duplicates the structure while each handle member shares its
  underlying resource exactly as if that handle were cloned directly.
  `(channel[int], int)` sent to a worker task gives the worker a tuple
  whose channel member is the same channel; handle operations stay
  internally synchronized by the runtime
- **owned members.** A tuple with at least one owned member (once
  `Box[T]` ships) is not `Copy`: it moves as a unit at every RFC 0043 move
  site — binding initialization, argument passing, `return`, `go` and
  `send` payloads, container insertion, struct and enum construction.
  Destructuring it moves the members out and consumes the source binding
  (`GOF3129`). Reading an owned member through `.N` is rejected; destructure
  the tuple or `clone` it first. These rules are fixed now so the `Box[T]`
  RFC cannot drift

`clone` follows the members: value members are O(1) CoW duplicates, handle
members share, owned members deep-copy. Tuple payloads crossing a task
boundary follow the RFC 0043 per-member rule: value members transfer when
unique and deep-copy deterministically when shared; handle members share;
owned members move unconditionally. A tuple has no destructor of its own;
members are destroyed at scope exit in reverse creation order, and handle
members close by final-reference close.

### Destructuring: one tuple-pattern grammar

One pattern grammar serves every destructuring position — the same
positional, exact-arity, bare-identifier style as today's match payload
bindings, generalized to nest:

- **binding statements.** `(a, b) = pair` and `mut (a, b) = pair` create or
  reassign bindings, one leaf per member. Each leaf independently follows
  today's single-name rules: a fresh name creates an immutable (or
  `mut`-declared) local; an existing `mut` name is reassigned; an existing
  immutable name is the existing immutable-reassignment error. `mut`
  applies to the whole pattern — every leaf is mutable — and no
  per-position `mut` exists. The right-hand side is one tuple-typed
  expression, fully evaluated before any target is written, so
  `(a, b) = (b, a)` swaps two `mut` bindings correctly

```gof text
point = (3, 4)
(x, y) = point
mut (w, h) = (10, 2)
(w, h) = (h, w)
```

- **loop bindings.** `for (a, b) in pairs:` destructures each element of
  an iterable of tuples. Leaves are immutable inside each iteration scope,
  like today's single loop binding

```gof text
fn total_area(boxes: list[(int, int)]) -> int:
    mut total = 0
    for (w, h) in boxes:
        total = total + w * h
    return total
```

- **match payload positions.** Each payload binding position in a match
  arm accepts a plain identifier (today's behavior, unchanged) or a tuple
  pattern that destructures a tuple-typed payload field in place. The
  parentheses are load-bearing there: `Gesture.Pan((dx, dy))` destructures
  one tuple payload field, while `Shape.Rect(w, h)` would bind two
  separate payload fields — construction and destructuring stay mirror
  images. Exhaustiveness, duplicate-arm, and arm-count rules are
  unchanged; tuple patterns refine payload positions only and never add
  arms. `Result` payloads compose the same way

```gof text
enum Gesture:
    Pan(delta: (int, int))
    Pinch(start: (int, int), end: (int, int))

fn is_grow(gesture: Gesture) -> bool:
    match gesture:
        Gesture.Pan((dx, dy)):
            return dx == 0 and dy == 0
        Gesture.Pinch((x0, y0), (x1, y1)):
            return x1 > x0 and y1 > y0
```

```gof text
import testing

fn divmod(a: int, b: int) -> Result[(int, int), RuntimeError]:
    if b == 0:
        return Result.Err(RuntimeError.Io("division by zero"))
    return Result.Ok((a / b, a % b))

test fn divmod_basics(t: TestContext):
    match divmod(17, 5):
        Result.Ok((q, r)):
            t.equal(q, 3, "quotient")
            t.equal(r, 2, "remainder")
        Result.Err(e):
            t.fail("divmod must not fail for a non-zero divisor")
```

Patterns carry no type annotations: member types are statically known from
the destructured value, so annotations would be redundant assertions.

**Why the parenthesized form.** The pattern mirrors the literal — `(1, "x")`
constructs, `(a, b)` destructures — and the identical grammar serves
statements, loops, and match payload positions. `mut (a, b)` distributes
obviously, and nesting stays uniform (`(a, (b, c)) = t`). Since `gof` has
no comma expressions, a bare target list `a, b = pair` would be a bare
comma form that exists nowhere else in the grammar, break the
construct/destructure symmetry, and force a `mut` distribution rule for
`mut a, b = ...` (Python's bare form works because Python tuples *are*
comma expressions — see Alternatives).

**Parser mechanics, layer discipline.** Statements may now begin with `(`:
the parser parses the parenthesized form and, when `=` follows, treats the
tuple literal as a pattern whose leaves must be identifiers or nested
tuple literals. `mut` may be followed by `(`. Both were parse errors
before. The parser checks only paren and comma well-formedness and pattern
leaf shape; every arity and type check lives in typed HIR (AGENTS.md §4 —
no parser-level typing hacks). The parser does emit `GOF3155` directly for
the degenerate shapes only it can see: `(x,)`, `()`, trailing commas, `(T)`
in type position, and single-item parentheses in pattern position.

### Element access

Postfix `.` followed by a canonical decimal integer literal (no leading
zeros, no sign) accesses a member of a tuple-typed value: `pair.0`,
`pair.1`, `frame.origin.0`, `min_max(nums).0`. It composes with field
access and call postfixes.

- indices are zero-based and checked against the tuple's arity in typed
  HIR; out-of-range and non-tuple targets are `GOF3157`
- the index must be a literal token: tuple arity is static, so runtime
  indexing cannot typecheck; `t[i]` stays list and dict indexing
- member assignment (`pair.0 = 5`) is not part of the surface — tuples are
  immutable values; construct a new tuple or destructure into `mut`
  bindings
- extracting an owned member through `.N` is rejected with a note to
  destructure or `clone`; the canonical-literal requirement is what keeps
  `pair.0` unambiguous against a future float literal (`1.5` lexes from a
  digit, `pair.0` from an identifier — the two never compete)

```gof text
struct Frame:
    origin: (int, int)
    size: (int, int)

fn right_edge(frame: Frame) -> int:
    return frame.origin.0 + frame.size.0
```

### Where tuples flow

Tuples are ordinary values and types in every existing position:

- function parameters: `fn offset(p: (int, int)) -> (int, int):`
- function returns, explicit and inferred: `return (lo, hi)` infers
  `(int, int)`; `expr?` unwraps a tuple `Result.Ok` payload
- type arguments: `list[(int, string)]`, `dict[(int, string)]`,
  `channel[(int, int)]`, `task[(int, string)]`,
  `Result[(int, string), RuntimeError]`
- struct fields and enum payload fields declared with tuple types
- fixture value types and typed fixture/test parameters
- container elements: list homogeneity unifies on the tuple type; existing
  container diagnostics apply unchanged
- `go` and `send` payloads and task join results, under the ownership
  rules above

### Equality and ordering

- operands must be the same tuple type — same arity, pairwise identical
  member types — mirroring same-type struct and enum comparisons
- `==` and `!=` are defined exactly when every member has equality
  semantics; lists, dicts, structs, enums, `Result` values, and tuples
  nest recursively per the existing rules
- `<`, `<=`, `>`, `>=` are defined exactly when every member is orderable;
  comparison is lexicographic, left to right, short-circuiting at the
  first unequal member, with each member compared in its own ordering
  domain
- capability violations reuse the existing comparison-operand diagnostic
  (`GOF3087`) — ordering a `(int, bool)` tuple, or comparing any tuple
  with a channel member. No new comparison code
- comparing different tuple types is a type error, not a capability
  question

### Rendering

Tuples render as `(member, member, ...)` recursively, so `print`,
`to_string`, `TestContext.equal`, and snapshots show `(1, x)` — string
members render unquoted, matching the existing list/dict rendering
convention (`[a, b]`, not `["a", "b"]`). There is
no tuple-to-`json` bridge: JSON helpers keep requiring `json` values and
converting a tuple is explicit `to_string` — no implicit coercion.

### Explicitly out of scope

- 1-tuples and 0-tuples — `unit` already covers the empty case
- variadic tuples, rest patterns `(a, ...rest)`, splat calls, and
  arity-generic functions
- tuple iteration (`for x in (1, 2, 3)`) and `len(...)` on tuples — arity
  is static and lists are the iterable sequence type
- `match` on a tuple scrutinee, and literal, wildcard, or guarded
  sub-patterns — those belong to the full-patterns Phase 1 slice
- pattern type annotations, member assignment, select-arm patterns

### Implementation mapping

- lexer: unchanged — no new tokens; tuple syntax composes existing tokens
- CST/AST: tuple literal, tuple type, tuple pattern, and element-access
  nodes; `for` bindings become patterns; the binding statement accepts a
  tuple pattern target
- HIR / typed HIR: `Type::Tuple(Vec<Type>)` with structural identity;
  arity and member checks (`GOF3155`, `GOF3156`); element-access checks
  (`GOF3157`); equality and ordering capability joins the existing rules;
  move tracking for owned-member tuples reuses the RFC 0043 checker
- MIR / backend (Phase 2): a tuple is the layout of its members — no
  header, no tag; destructuring and `.N` lower to member moves and loads
- evaluator (bootstrap): `Value::Tuple(Vec<Value>)` with eager duplication
  as an implementation detail, per RFC 0043's evaluator note — the
  observable semantics are identical on both targets

## Alternatives Considered

### Bare comma targets: `a, b = pair`

Rejected. Python's bare form exists because Python tuples are comma
expressions and the parentheses are optional on both sides. `gof`
deliberately has no comma expressions: values are written `(1, "x")`, so
bare targets would destructure without the parens that construct — an
asymmetry, plus a statement-level bare comma list found nowhere else in
the grammar, plus a forced `mut` distribution rule for `mut a, b = ...`.
The parenthesized pattern answers all three and keeps one grammar for
values, targets, and match payload positions. `gof` borrows Python's
readability, not its comma-tuple baggage. Allowing both forms was also
rejected: two spellings for one statement.

### Destructure only through `match`

Rejected. `match` requires an enum or `Result` scrutinee, and this RFC
keeps it that way; extracting from a plain tuple would then require
wrapping it in an enum or `Result` first — ceremony for the most common
case. The match payload binding style is not discarded either: it is the
pattern grammar adopted here, extended to nest.

### Single-element and empty tuple forms

Rejected. `(x,)` and `()` are syntactically distinguishable from grouping,
so the ban is a design choice: a single-element tuple is a wrapper with
worse ergonomics than the bare value or a one-field struct (which at
least names the field), it is a typo magnet (`(x,)` for `(x)`), and arity
`>= 2` keeps every pattern and destructuring rule total and simple.
`()` as a unit literal was rejected because `unit` already exists — a
second spelling of one value is drift, and 0-tuple patterns buy nothing.

### Tuple indexing `t[i]`

Rejected. Static arity means only compile-time-constant indices can
typecheck; runtime indexing would need dependent typing. Reusing `[]`
would also conflate tuple positions with list indexing. `.N` with a
literal index is the checkable subset, in the Rust and Swift form.

### No element access at all (destructure-only tuples)

Rejected. Reading one member of a returned pair (`report.0`,
`pair.1`) is the common case; forcing full destructuring — and naming the
ignored member, since there are no wildcards — is ceremony. `.N` is a
typed-HIR extension of existing postfix field access, small and
checkable, and cannot be misused dynamically.

### Variadic or arity-generic tuples

Rejected. Rest types `(T, ...)`, splat calls, and arity-polymorphic
functions need a variadic type system `gof` does not have and should not
grow for this. Fixed arity is the point: homogeneous variadic data is
`list[T]`, labeled product types are structs.

### Tuple scrutinee in `match`

Rejected for this RFC. A tuple has exactly one total shape, so
variant-style dispatch has nothing to dispatch on and destructuring
already covers it. Literal sub-patterns like `(0, b):`, wildcards, and
guards are the separate full-patterns slice and must not land
half-designed here.

### Structs only, no tuples

Rejected. Multi-return and coordinate-style pairs would require one-off
struct declarations whose names carry no domain meaning. Tuples are the
anonymous fixed record every ancestor language converged on, and RFC 0043
already commits to them as a value taxon.

## Compatibility Impact

- every existing program keeps compiling. `(expr)` grouping is unchanged
  (the `parse_primary` parenthesized path); `(a, b)` was already a parse
  error today because the parser requires `)` after a grouped expression
  and no comma expressions exist; parenthesized types, patterns, and
  statement targets were parse errors; `expr.0` was a parse error — no
  existing meaning can change
- keeping `(x)` as grouping is a compatibility requirement: 1-tuples
  would silently change `f((x))` from a one-argument call into a tuple
  argument
- no new tokens and no new reserved words; tuple syntax composes existing
  tokens
- ownership contracts for existing taxa are untouched — tuples introduce
  no new taxon, they are RFC 0043 composites
- some previously generic parse errors (redundant parentheses in type or
  pattern position) now produce a precise code — a strict improvement
- public surfaces that must update with the implementation:
  `spec/language-v1.md`, `spec/diagnostics.md`, `docs/site/`, `examples/`,
  `roadmap.md`, and the `tools/vscode-gof` grammar and snippets

## Diagnostics Impact

This RFC adds three error codes:

- `GOF3155`: invalid tuple arity or shape — single-element or empty tuple
  forms (`(x,)`, `()`, `(T,)`, `(T)`, `(p)` in pattern or target
  position), trailing commas, and destructuring or match-pattern arity
  that does not match the tuple's member count. Fix-it: remove the
  trailing comma or parentheses, or add or remove bindings
- `GOF3156`: tuple destructuring against a non-tuple value — a
  destructuring pattern applied to a value or payload field that is not
  a tuple (including nested leaves). The note names the expected and
  found member types
- `GOF3157`: invalid tuple element access — element access on a
  non-tuple value or an index outside the tuple's arity (extraction of
  an owned member joins with RFC 0043's move checker when owned taxa
  land)

Implementation amendment (review-verified): where tuple member types
flow through EXISTING position machinery — annotated bindings,
parameters, returns, type arguments, struct fields, enum payload
construction, and container elements — the existing position codes
fire (`GOF3013` for type mismatches, `GOF3017` for list-element
mismatches), with the tuple member types named in their notes. The
three new codes own the tuple-SPECIFIC shapes above instead of
duplicating position checks. Comparing different tuple types likewise
reuses `GOF3087` with both types in the note (the "type error, not a
capability question" wording above is satisfied by the note carrying
the mismatched types; no separate code is allocated).

Existing codes keep their contracts: comparison-capability violations
reuse `GOF3087`; consuming an owned-member tuple through destructuring
reuses RFC 0043's `GOF3129`; malformed pattern leaves (a non-binding
expression in target position) are parser-level `GOF2001`/`GOF2002` errors
with tuple-specific messages. `spec/diagnostics.md` gains the three new
codes when the tuple slice lands.

## Test Plan

- parser/AST unit tests: tuple literal, type, and pattern parsing;
  grouping preserved (`(x)` unwraps to `x`, `f((x))` keeps call arity);
  nesting; the degenerate forms (`(x,)`, `()`, trailing comma, `(T)`,
  `(p)`) reported as `GOF3155`; statement-level destructuring including
  `mut (`; `for` pattern bindings; match payload patterns; `.N` postfix
  including canonical-literal rejection (`t.01`)
- typed-HIR tests: structural type identity including order sensitivity
  and nesting; `GOF3155` arity across destructuring, patterns, and
  literals; `GOF3156` member compatibility across every integration
  position (parameters, returns, type arguments, struct fields, enum
  payloads, fixture types, container unification); `GOF3157` range and
  operand checks; equality and ordering capability propagation with
  `GOF3087` negatives (channel member, ordering over `(int, bool)`)
- evaluator and conformance: construction, destructuring binding semantics
  (fresh creation, `mut` reassignment, immutable-reassignment error), swap
  on `mut` bindings, match destructuring with nested and mixed
  pattern/binding arms, `for` pattern iteration, element access reads,
  lexicographic ordering, rendering, snapshot output
- RFC 0043 boundary tests: a tuple payload crossing a channel with a
  handle member (sharing observable), value-member copy-at-boundary when
  shared, unique-transfer where observable
- fixtures: positive and negative compile fixtures for each new code,
  following the existing fixture discovery surfaces
- docs and examples: an `examples/tuples.gof` walkthrough, spec and
  `docs/site` sync, doctests for the paren rule and the destructuring
  forms
- VS Code extension (AGENTS.md §9): grammar and snippets for literals,
  types, patterns, and `.N`; `npm test` and `npm run package` verified

## Benchmark Impact

- no new runtime construct: a tuple is the layout of its members — no
  header, no tag, no vtable, no required allocation
- Phase 2 gates (native): value-member tuples pass in registers or on the
  stack and never force heap allocation; destructuring and `.N` lower to
  direct member moves and loads; a tuple return matches an equivalent
  two-field struct return in codegen
- RFC 0043 gates apply unchanged: duplication is O(1) per CoW member;
  unique value-member tuples take the task-boundary transfer fast path
- compile time: structural unification is bounded by arity; no type-class,
  variance, or variadic machinery
- no zero-cost claim is accepted without a recorded number (VISION
  honesty rules)
