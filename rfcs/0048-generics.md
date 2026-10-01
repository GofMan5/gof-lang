# RFC 0048: Generics — Parameterized User Types and Functions

## Summary

Introduce user-defined generic declarations over explicit type parameters:

- generic functions through `fn name[T: SomeProtocol](x: T) -> T:`
- generic nominal types through `struct Pair[T]:` and `enum Option[T]:`
- receiver methods on generic structs reuse the struct's type parameters
- constraints reference protocols from RFC 0044 "Protocols" (drafted in
  parallel); a constraint fixes exactly which operations the body may
  perform on values of that parameter type
- call sites instantiate by inference where the arguments and expected type
  determine a unique substitution, and explicitly through `name[int](...)`
  and `Name[int]` otherwise
- the typed-HIR checker verifies each generic declaration **once** with its
  parameters abstract, and each instantiation site through cheap structural
  checks: arity, closedness, handle exclusion, cached protocol conformance,
  inference
- dispatch follows ADR 0004: dev builds compile one type-erased body per
  generic declaration and dispatch through witness tables with instance
  caching at module boundaries; release builds monomorphize in typed HIR
- generic-typed values follow the conservative ownership rule: bodies check
  them as move-only (RFC 0043), and consuming sites relax to duplication
  for `Copy`-taxon instantiations at lowering
- type arguments may never resolve to handle types

Builtin parameterized annotations (`list[T]`, `dict[T]`, `channel[T]`,
`task[T]`, `Result[T, E]`) are unchanged by this RFC; the recommendation is
to migrate them onto this machinery in a follow-up RFC.

Constraints: RFC 0044 "Protocols". Dispatch: ADR 0004. Value taxa: RFC 0043.

## Problem

RFC 0013 shipped parameterized *builtin* annotations and explicitly deferred
full user-defined generics. That deferral has run out:

- user data structures cannot abstract over their element types: a pair of
  ints and a pair of strings are two struct declarations today, or the code
  drops to untyped `json` and loses the static contract
- Phase 1 needs `Option[T]`, tuples, and collection types, and Phase 4 needs
  stdlib collections — all of them are generic declarations that need one
  decided representation, inference rule set, and checking model
- ADR 0004 already fixed the dispatch strategy — witness tables in dev,
  monomorphization in release — but the language semantics that dispatch
  implements are still undecided: what a constraint guarantees, what is
  checked per declaration versus per instantiation, and how instantiation
  interacts with the value taxa. Without an RFC, dispatch decisions leak
  into the checker ad hoc
- RFC 0043 fixed three value taxa with strictly local analysis; generics
  must compose with that model without interprocedural analysis,
  per-instantiation re-checking, or a borrow checker

## Proposed Change

### Declarations

Functions, structs, and enums declare an ordered type-parameter list in
brackets directly after the declared name:

```gof
struct Pair[T]:
    first: T
    second: T

fn identity[T](value: T) -> T:
    return value

fn largest[T: Ordered](items: list[T]) -> Result[T, RuntimeError]:
    if len(items) == 0:
        return Result.Err(RuntimeError.EmptySequence("largest of empty list"))
    mut winner = first(items)?
    for item in items:
        if winner < item:
            winner = item
    return Result.Ok(winner)

enum Option[T]:
    Some(value: T)
    None
```

Rules:

- each type parameter may declare at most one protocol constraint through
  `T: SomeProtocol`; constraints are optional — an unconstrained parameter
  still supports the universal operations below
- parameter names must be unique inside their bracket and must not collide
  with any builtin, struct, enum, or protocol name visible in the module
  graph
- constraint protocols resolve across the module graph like type names and
  follow RFC 0044 "Protocols"
- fields, payload fields, parameter types, and return types may reference
  the declaration's parameters, including nested applications such as
  `list[Pair[T]]`
- inside generic declarations, builtin parameterized annotations may
  reference the declaration's parameters; their builtin semantics are
  unchanged
- `test fn`, `fixture(scope) fn`, and methods may not declare their own
  type-parameter lists
- methods on generic structs reference the struct's parameters in scope;
  the receiver parameter must be exactly the struct type applied to the
  struct's own parameter names in declaration order:

```gof
struct Pair[K, V]:
    first: K
    second: V

fn Pair.swapped(self: Pair[K, V]) -> Pair[V, K]:
    return Pair(self.second, self.first)
```

- other annotations in a method may apply the parameters in any position,
  as the `Pair[V, K]` return type above shows
- `mut` receivers on generic structs follow the RFC 0043 receiver rules
  unchanged
- generic functions may rely on return-type inference exactly like
  non-generic functions, with parameters treated as abstract atoms
- closures created inside generic declarations may not capture
  type-parameter-typed values, and generic declarations may not use
  closure types mentioning the declaration's parameters in any position —
  signatures, struct fields, or enum payloads — the RFC 0047 v1
  restriction: the taxon of a `T`-typed capture is instantiation-
  dependent, per-`T` capture layouts would force dev-profile
  monomorphization, and witness tables carry method slots only
  (violation reports `GOF3146`)

Protocol names in the examples are illustrative; the concrete protocol
surface is owned by RFC 0044.

### Type parameters and constraints

A constraint fixes the capability set available to the body. Values of
parameter type `T` support:

- universal operations: binding, argument passing, container and field
  insertion, generic construction over `T`, `return`, and `clone`
- the members required by the constraint's protocol, resolved by name and
  dispatched indirectly
- operators (ordering, equality, and any future operator-bearing protocols)
  only when the constraint's protocol requires the corresponding witness

Any operation beyond the constraint — a member the protocol does not
require, an operator with no required witness — is a declaration-time error
(GOF3150). `fn identity[T]` needs no constraint because it only moves a
value through; `fn largest[T: Ordered]` needs one because it compares.

### Instantiation

Generic functions, struct constructors, and enum variant constructions are
instantiated at call sites; type applications in annotations instantiate
generic types. Two forms:

- **inferred** — no explicit bracket. Type arguments follow from
  unification against anchors:
  - argument expression types against declared parameter types
    (`list[T]` against `list[int]` gives `T = int`)
  - constructor arguments against field and payload types
  - the declared expected type at annotated bindings, typed mutable
    bindings, and `return` statements against the declared return type
    (`return Option.None` inside a function returning `Option[int]`)
- **explicit** — `name[int](...)` for call targets and `Name[int]` in
  annotations. The list must be complete: every declared parameter, in
  order. Partial lists and placeholders are not accepted in this RFC

Disambiguation from indexing (the RFC 0047 interplay): an
`Identifier[bracket](...)` sequence parses as explicit type application
exactly when the bracket contents parse as a **type list** (type names,
possibly nested `Name[...]`); every other bracket content — integer
literals, general expressions — parses as indexing followed by a call,
which is legal since RFC 0047 makes closures container elements —
indexing a list of closures and calling the result (`fns[0](arg)`) is
ordinary indexing. In the rare conflict where a bracket identifier names
both a type and an in-scope value, the type-list parse wins and the
value use reports with a rename hint. A shared parser regression test
with RFC 0047 pins both forms: `fns[0](arg)` is indexing;
`identity[int](41)` is type application.

Rules:

- inference runs only when no explicit list is supplied; explicit arguments
  are never re-inferred
- unification is first-order and structural: parameters are extracted only
  from matching constructors (`Pair[K, V]` against `Pair[int, string]`);
  `dict[T]` stays string-keyed; non-matching constructors contribute no
  anchor
- all anchors are gathered and must agree on one unique substitution per
  parameter; a missing anchor or conflicting candidates is GOF3149 with
  candidates rendered in declaration order and a fix-it suggesting the
  explicit form
- inference is local: anchors never propagate into callee bodies or through
  nested calls, keeping diagnostics deterministic
- instantiations inside another generic declaration may pass the enclosing
  parameters as type arguments (`return Pair(value, value)` inside
  `fn pair_of[T](value: T) -> Pair[T]:`); these are checked structurally
  and re-resolved per outer instantiation
- type arguments must be closed types outside generic declarations
- generic types must be fully applied everywhere they appear; an unapplied
  generic type is GOF3147
- explicit type application is recognized only as `Identifier[args](...)`
  call targets and `Name[args]` annotations; method calls cannot carry
  explicit type arguments — the receiver's instantiation provides them
- `go generic_fn(args)` spawns with the same call-site inference; the task
  value carries the instantiated return type

Type-argument validity, per argument:

- the argument must not resolve to a handle type — `channel`, `task`,
  `cancel_token`, and the shipped opaque runtime handle types (GOF3148).
  Composites containing handle payloads (`list[channel[int]]`) remain
  allowed: RFC 0043's composite rules already define their duplication and
  sharing behavior, and a generic body never performs handle-specific
  operations on `T`
- the argument must satisfy the parameter's constraint through RFC 0044's
  conformance relation, memoized per `(type, protocol)` per module
  (GOF3148)
- a type parameter passed as a type argument satisfies a constrained
  position only when its own constraint is the same protocol (identity);
  refinement-based subsumption is deferred until RFC 0044 settles protocol
  refinement (GOF3148 otherwise)

```gof
fn main() -> int:
    inferred = identity(41)
    explicit = identity[int](41)
    return inferred + explicit

fn positive_or_none(count: int) -> Option[int]:
    if count < 0:
        return Option.None
    return Option.Some(count)

fn report_all[T](tasks: list[task[T]]) -> list[T]:
    mut values: list[T] = []
    for handle in tasks:
        values = append(values, await handle)
    return values
```

`report_all` shows the handle boundary: `T` binds the join payload type;
the task handle itself is never a type argument — `Pair[task[int]]` is
GOF3148. `positive_or_none` shows both anchor kinds: the payload argument
anchors `Option.Some(count)` and the declared return type anchors
`Option.None`.

### Instantiation identity and assignment

- instantiation is nominal and invariant: `Pair[int]` and `Pair[string]`
  are unrelated types; two occurrences of `Pair[int]` are the same type
- assignment, parameter passing, and return compatibility between
  instantiations require exact argument identity and report through the
  existing mismatch diagnostics
- equality of a generic struct or enum instantiation follows the current
  member rule: available at an instantiation exactly when every substituted
  member has explicit equality semantics; inside a generic body it requires
  the constraint to require equality, checked once

### Ownership interaction

RFC 0043's taxa are per instantiation, so generic bodies are checked under
one conservative rule:

- inside a generic declaration, values of generic-parameter type are
  treated as move-only in every consuming position from RFC 0043's
  classification; use after consumption is GOF3129 with the same fix-it
  (`clone(value)`)
- this is the most restrictive taxon, so it is sound for every legal
  instantiation and needs no interprocedural or per-instantiation analysis
- at instantiation, consuming sites relax: for `Copy`-taxon arguments they
  lower to duplication (O(1) under copy-on-write, exactly RFC 0043's
  contract); for owned-taxon arguments they stay moves; for composites with
  handle payloads they duplicate with shared payloads
- handle taxa are excluded as type arguments, so no generic body can depend
  on aliasing behavior

### Typed-HIR representation

Generic declarations and instantiation sites extend the existing typed-HIR
shapes (conceptual sketch, not final code):

```rust
pub enum Type {
    // builtin variants unchanged
    Struct(String, Vec<Type>),            // args empty for non-generic structs
    Enum(String, Vec<Type>),
    Param { name: String, index: usize }, // valid only inside its own declaration
}

pub struct GenericParam {
    pub name: String,
    pub constraint: Option<ProtocolRef>,  // RFC 0044
}

// TypedFunction, TypedStruct, TypedEnum gain:
//     generic_params: Vec<GenericParam>
// Call, struct-constructor, and variant-construction expressions gain:
//     type_arguments: Vec<Type>            // resolved after inference
```

- nominal variants carry an ordered argument vector that is empty for
  non-generic declarations, so every existing `Type` value keeps its
  meaning, and `display_name` renders `Pair[int]` through the existing
  nesting path
- the parameter variant is produced only while checking the declaration
  that binds it; it never escapes into a closed type
- each module gains an **instantiation table** keyed by
  `(declaration symbol, ordered type arguments)` recording:
  - the substitution map used for release specialization
  - the conformance witness records per constrained parameter
  - the type-erased call symbol used by the dev profile
  - whether a release specialization was emitted
- the instantiation table is part of the module's boundary section of the
  serialized IR cache (ADR 0004), so incremental rebuilds reuse
  instantiation records instead of recomputing them

### Checked once per declaration versus per instantiation

The compile-budget contract behind ADR 0004 is made structural here: body
checking happens exactly once per declaration; instantiation never
re-checks bodies.

Verified once per generic declaration, with parameters abstract:

- parameter-list well-formedness: unique names, no collisions with known
  type names, constraints resolving to protocols (GOF3146)
- arity of every type application appearing inside the declaration,
  including nested applications and method receivers (GOF3147)
- receiver parameter form on methods of generic structs (GOF3146)
- body typechecking: each `T` is an opaque type whose capability set is the
  constraint's members plus the universal operations; anything beyond is
  GOF3150
- return-path compatibility with the declared or inferred return type
- `match` exhaustiveness inside generic bodies, including over generic
  enums and `Result` with parameter-typed payloads
- move and consumption discipline under the conservative rule (GOF3129)
- constraint identity at parameter-as-argument sites inside the body
  (GOF3148)
- the termination contract: a declaration may instantiate itself only with
  type arguments identical, up to parameter renaming, to its own; direct
  self-instantiation with growing arguments is GOF3146

Verified per instantiation site, all cheap and cached:

- supplied or inferred type-argument count equals the declaration's
  parameter count (GOF3147)
- each type argument is closed in context; enclosing parameters are legal
  only inside generic declarations
- no type argument resolves to a handle type (GOF3148)
- protocol conformance per `(type argument, constraint)` through RFC
  0044's relation, memoized per module (GOF3148)
- inference resolution when no explicit list is supplied (GOF3149)
- call arguments, constructor arguments, and annotated targets are
  compatible with the substituted signature — the existing call
  compatibility checks with the substitution applied, reporting through
  the existing mismatch diagnostics

Never re-verified per instantiation: body well-formedness, capability
usage, move discipline, exhaustiveness, return compatibility. Instantiation
count scales as table entries, never as re-checking.

### Dispatch and lowering

Dev profile (ADR 0004 — witness tables):

- each generic declaration compiles to **one** type-erased body; dev never
  clones generic bodies
- values of generic-parameter type flow as `(type descriptor, payload)`
  pairs in the uniform erased layout; struct instantiations use the erased
  layout for parameterized fields
- every instantiation passes, per type parameter, a type descriptor
  (layout, clone, destroy, and any constraint-required witnesses such as
  equality) plus the protocol witness table for its constraint; RFC 0044
  owns the witness-table layout
- protocol-member calls on `T` inside the body lower to witness-table
  indirect calls
- instance caching at module boundaries: the instantiation record for
  `(symbol, args)` is materialized once in the importing module's boundary
  section of the binary IR cache and reused by every later use in that
  module; generic struct instantiations imported across modules share the
  cached descriptor record so field access stays consistent
- instantiation count cannot grow dev compile time beyond table inserts

Release profile (ADR 0004 — full monomorphization):

- typed HIR closes over the instantiation table: every reachable
  `(declaration, args)` pair is specialized by applying the recorded
  substitution to a clone of the declaration, before MIR lowering
- no re-typechecking: the declaration was verified once, and the
  per-instantiation records already hold the conformance results
- witness indirections fold to direct calls; unreachable instantiations
  are dropped
- consuming sites over generic-typed values relax from moves to copies when
  the substituted taxon is `Copy` — semantics-preserving under RFC 0043's
  duplication contract
- instantiation expansion enforces a depth budget; a growing chain
  (indirect recursion through instantiations) reports GOF3146 with the
  chain rendered in notes. In dev the same program is ordinary runtime
  recursion and fails through the runtime stack guard like non-generic
  recursion

Bootstrap evaluator: implements the same instantiation rules dynamically —
closedness, handle exclusion, conformance lookups, inference — so
conformance tests run against both targets. The evaluator is the
conformance oracle, never a performance baseline.

### Relationship to builtin parameterized annotations

`list`, `dict`, `channel`, `task`, and `Result` keep their current
resolution, arity diagnostics (GOF3067), and match behavior in this RFC.
The new machinery is deliberately uniform with them so they can migrate.

Recommendation: migrate the builtins onto this system — as a follow-up RFC,
not here:

- after migration, `Result[T, E]` is a builtin generic declaration with two
  parameters; `list[T]`, `dict[T]`, `channel[T]`, and `task[T]` are builtin
  generic declarations with pinned semantics (`dict[V]` stays string-keyed;
  `task[T]`'s parameter names the join result type)
- migration collapses the two parameterization paths in the checker and
  gives builtin annotations and user generics one instantiation, inference,
  and diagnostics story
- it is deferred because the builtin paths are load-bearing across the
  whole shipped surface — diagnostics text, match lowering, evaluator
  behavior — and migrating them here would double regression risk for zero
  new user-facing capability
- `Box[T]` and `Shared[T]` (RFC 0043 planned stdlib) should land directly
  as generic builtin declarations once migration is done

Until then, builtin parameterized annotations and user generics compose by
nesting only: `list[Pair[int]]` and `Pair[Result[int, RuntimeError]]` are
valid; `Pair[channel[int]]` is GOF3148 because the argument is a handle
type.

### Non-goals

- variance or subtyping between instantiations — compatibility is nominal
  and invariant
- higher-kinded types — type arguments are complete types; `Pair[list]` is
  invalid
- generic protocols — protocols stay non-generic (RFC 0044)
- generics over handle types — handle taxa are excluded as type arguments
- partial explicit type-argument lists and inference placeholders
- associated types and inference beyond the local anchors defined above
- generic `test fn` and `fixture(scope) fn` declarations
- default type arguments, specialization, overlapping implementations
- migrating builtin parameterized types — follow-up RFC, recommended above

## Alternatives Considered

### C++-style templates checked per instantiation

Rejected. Per-instantiation body checking re-runs the whole typecheck at
every call site, spending the dev compile budget ADR 0004 protects, and
reports errors at call sites instead of declarations. Check-once with
abstract parameters keeps errors at the declaration and instantiation
cheap.

### Java-style erasure with no monomorphization

Rejected. A uniform erased representation in the release profile forfeits
the C-grade cost target: boxed generic values and indirect calls that can
never fold. ADR 0004's dual profile keeps dev fast and release direct.

### Monomorphization in both profiles

Rejected. ADR 0004 decided this: witness tables with instance caching at
module boundaries in dev so generics never eat the dev compile budget;
monomorphization only in release.

### Turbofish syntax `name::<T>(...)`

Deferred, not rejected. The plain bracket form `name[T](...)` is kept
because the disambiguation from indexing is already decided (see the
Instantiation section): bracket contents that parse as a type list make
the sequence an explicit type application, and every other bracket
content — including `fns[0](arg)`, valid since RFC 0047 makes closures
container elements — parses as indexing. A turbofish form would add
surface without removing an existing ambiguity; if a future feature ever
makes type-list-versus-expression ambiguity genuinely unavoidable, the
turbofish form is the reserved escape hatch.

### Mandatory constraints on every type parameter

Rejected. A required no-op marker protocol is ceremony without safety: the
capability discipline is enforced per operation (GOF3150) either way, and
storage-only declarations such as `struct Pair[T]:` legitimately need no
constraint. If RFC 0044 ships a marker protocol, it can be used to state
intent explicitly.

### Migrating builtin parameterized annotations in this RFC

Rejected. Risk isolation: builtin parameterization is load-bearing across
the shipped surface. The migration is recommended as a follow-up once this
machinery is proven.

### Variance-based generics

Rejected. Variance buys coercions between instantiations at the price of a
subtyping relation woven through the whole checker. Nominal invariance is
explicit, cheap to check, and matches the rest of the type system.

## Compatibility Impact

- purely additive: type-parameter brackets on user declarations are errors
  today, and `Identifier[type_list](...)` cannot be a valid index-then-call
  sequence in current gof, so no existing program changes meaning
- builtin parameterized annotations, their arity diagnostics (GOF3067), and
  `Result` match behavior are unchanged
- bare non-generic struct and enum names remain valid annotations; generic
  types must be fully applied everywhere
- methods on non-generic structs are unchanged
- generic declarations participate in the existing module-graph
  duplicate-name rules unchanged
- fixture return values may be generic instantiations (a
  `fixture(test) fn` returning `Pair[int]`); only declaring generic test
  and fixture functions is out of scope
- the VS Code extension must learn the bracket forms and call-site type
  application (AGENTS.md §9)
- `spec/language-v1.md` binding rules, `spec/diagnostics.md`, examples, and
  `roadmap.md` sync when the typed-HIR pass lands

## Diagnostics Impact

This RFC adds exactly five codes, all errors, each following the
diagnostics contract (code, severity, span, message, note, fix-it):

- `GOF3146`: invalid generic declaration contract — duplicate
  type-parameter names, collisions with known type names, unresolved
  constraint protocols, type parameters on `test fn`, `fixture(scope) fn`,
  or methods, malformed receivers on generic structs, non-terminating
  instantiation expansion, and closure captures or closure types
  violating the RFC 0047 generic restriction (T-typed captures; closure
  types mentioning the declaration's parameters in any position)
- `GOF3147`: type-argument arity mismatch — wrong counts at annotations,
  call sites, and constructors, including explicit arguments on non-generic
  targets, unapplied generic types, and explicit arguments on method calls
- `GOF3148`: type argument does not satisfy the parameter's requirements —
  protocol conformance failure, handle-taxa arguments, and
  parameter-as-argument constraint mismatches; the note states which
  requirement failed
- `GOF3149`: cannot infer a unique type argument — missing anchors or
  conflicting candidates; the fix-it suggests the explicit instantiation
  form
- `GOF3150`: operation not permitted by the type parameter's constraint —
  protocol-member or operator use without a required witness, reported at
  declaration check

Interactions with existing codes (no renumbering, note text only):

- `GOF3129` (RFC 0043) applies inside generic bodies under the conservative
  move-only rule
- `GOF3012` and `GOF3067` notes update: user types can now be
  parameterized, so the "only builtin container and task annotations are
  parameterized" note is no longer accurate; both codes keep their
  semantics for builtins and unknown names
- call-site compatibility after substitution reports through the existing
  mismatch diagnostics (for example `GOF3013`), with instantiation names
  rendered through the existing display path

`spec/diagnostics.md` gains the five codes when the typed-HIR pass lands.

## Test Plan

- parser tests: bracket forms on `fn`, `struct`, and `enum`; mixed
  constrained and unconstrained parameters; call-target type application;
  malformed lists
- ambiguity regression: `x[0](...)` and `x[index](...)` parse as indexing,
  never as generic application
- typed-HIR unit tests:
  - declaration checks: every `GOF3146` and `GOF3147` subcase
  - abstract body checking: every `GOF3150` case (method, operator, nested
    member)
  - conservative move rule: `GOF3129` inside generic bodies
  - instantiation checks: `GOF3147`, `GOF3148`, and `GOF3149` subcases
    (arity, handle exclusion, conformance, missing and conflicting anchors)
  - inference: argument anchors, expected-type anchors, nested unification
    (`Pair[Pair[int]]`), parameter-as-argument inside generic bodies,
    deterministic candidate rendering
- positive and negative fixtures: programs that must compile and programs
  that must fail with each new code
- generic examples: `examples/generic_functions.gof` and
  `examples/generic_pairs.gof` covering struct, method, and enum forms
- evaluator conformance: identical observable behavior for the same
  programs before and after the checks land
- dev/release equivalence: identical observable behavior between
  witness-table dispatch and monomorphized specialization, gating the ADR
  0004 split once Phase 2 lands
- VS Code extension: grammar and snippets for the new forms; `npm test` and
  `npm run package` (AGENTS.md §9)
- docs sync: spec binding rules, diagnostics list, examples, roadmap

## Benchmark Impact

Gates anchored at Phase 2, numbers before claims (VISION honesty rules):

- dev compile budget: generic-heavy module rebuild wall-clock stays within
  the pinned small-rebuild budget — instantiation count must scale as table
  inserts, never as re-checking or re-lowering
- witness dispatch overhead: indirect protocol-member call cost in dev
  versus release direct calls, measured
- release zero-cost gate: monomorphized generic call versus a hand-written
  monomorphic equivalent on scorecard workloads; target is parity within
  noise
- code size: instantiation growth and dead-instantiation elimination rate
  in release builds
- the evaluator is never a performance baseline (ADR 0004)
