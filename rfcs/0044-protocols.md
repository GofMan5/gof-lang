# RFC 0044: Static-Dispatch Protocols

## Summary

Introduce the `protocol` surface that is already reserved in the lexer
(`TokenKind::Protocol`) and listed in the spec v1 syntax surface, as gof's
trait-like interface mechanism:

- top-level `protocol Name:` blocks declaring **method requirements**:
  signature-only `fn` lines whose receiver parameter is annotated `Self`,
  the placeholder for the conforming type
- explicit conformance through a block-less top-level declaration
  `impl Circle: Shape`, checked at compile time against the module's
  declared methods — structural similarity without an `impl` never conforms
- **protocol-constrained generic functions** as the only v1 use of
  conformance: `fn describe[T: Shape](shape: T) -> string:` — static
  dispatch in both build profiles per ADR 0004: witness tables in the dev
  profile, monomorphization in the release profile
- **no protocol-typed values in v1**: protocol names are legal only in
  protocol declarations, `impl` declarations, and generic bounds; every
  other position is a compile error. Existentials get an honest tradeoff
  and a revisit trigger in Alternatives Considered, not a silent promise

This RFC is the Phase 0 protocol gate artifact named in VISION.md. It fixes
the conformance contract that the Phase 1 generics RFC builds on. It does
not deliver unbounded generics, generic structs or enums, associated types,
protocol inheritance, or async requirements.

## Problem

gof has exactly one polymorphism axis today: the concrete type. Methods
resolve by receiver type (`value.method(...)` requires a known method on
that exact struct), so cross-cutting behavior can only be expressed by:

- duplicating helper functions per concrete type — no shared algorithm over
  "anything with an `area`"
- untyped escape hatches (`json` values, unannotated parameters) — the
  dynamic chaos gof exists to refuse
- closed `enum` sum types, which couple the abstraction to a variant set
  fixed at declaration time and known to the abstraction's author

The language already promises otherwise: `protocol` lexes as a keyword but
has no parser, type, or codegen meaning — a reserved word that is a promise
with no contract. Phase 1 needs generics; generics need a bound story; the
bound story is the protocol contract. Without this RFC:

- the generics RFC has no constraint surface to design against and risks
  inventing one under delivery pressure (layering debt)
- stdlib RFCs cannot ship shared behavioral contracts without per-type
  duplication or `json`-typed dynamic dispatch
- "zero-cost abstraction" stays an untestable claim, which the VISION
  honesty rules forbid

## Proposed Change

### End-to-end shape

```gof
protocol Shape:
    fn area(self: Self) -> int
    fn name(self: Self) -> string

struct Circle:
    radius: int

struct Square:
    side: int

fn Circle.area(self: Circle) -> int:
    return self.radius * self.radius * 3

fn Circle.name(self: Circle) -> string:
    return "circle"

fn Square.area(self: Square) -> int:
    return self.side * self.side

fn Square.name(self: Square) -> string:
    return "square"

impl Circle: Shape
impl Square: Shape

fn describe[T: Shape](shape: T) -> string:
    return shape.name() + " " + to_string(shape.area())

fn main() -> int:
    print(describe(Circle(2)))
    print(describe(Square(3)))
    return 0
```

Methods keep today's declaration form, conformance is one explicit line,
generic functions carry bounds, and dispatch is static in both profiles.

### Protocol declarations

```gof
protocol Name:
    fn requirement(receiver: Self[, param: Type, ...]) -> RetType
    fn mutating(mut receiver: Self[, ...]) -> RetType
```

Rules:

- `protocol Name:` at module top level, followed by an indented block of
  method requirements; the block must contain at least one requirement
  (empty marker protocols are rejected — there is no `pass`, and marker
  protocols have no use without existentials)
- a requirement is a signature-only `fn` line: no body, no trailing colon
  (the trailing colon is gof's block-introducer; a requirement has nothing
  to introduce), terminated by the newline
- the first parameter is the receiver: annotated `Self`, name free — the
  same naming rule as concrete receivers today
- `mut` receiver requirements use `mut receiver: Self`
- `Self` is a type placeholder valid **only** inside a protocol block, only
  as the direct annotation of a parameter or as the return type; it may not
  nest inside parameterized annotations
- every other type in a requirement signature must be a type the
  function-signature annotation grammar accepts: builtins, parameterized
  builtins, structs, enums, and — with the RFCs in this same set —
  `Option[T]`, tuple types, and closure types
- requirement names are unique within the protocol; protocol names join
  the top-level namespace and conflict with struct, enum, and function
  names under the existing top-level conflict rules
- no `test`, `fixture`, or receiver-qualified forms inside a protocol block

Why this shape, against the existing method declaration syntax
`fn TypeName.method(self: TypeName)`:

- the concrete method form names its owner twice — the qualifier and the
  receiver type. A requirement has no owner to name: the conforming type is
  unknown, so both owner slots collapse into one placeholder, the receiver
  annotation. The requirement form is literally the method form with the
  owner elided and the receiver type generalized:
  `fn Circle.area(self: Circle) -> int` ↔ `fn area(self: Self) -> int`
- the unqualified name matches how the requirement surfaces at use sites
  (`shape.area()`). A qualified `fn Shape.area(...)` form would falsely
  suggest the protocol owns or provides a body; protocols only *require*
  methods, and structs provide them through the existing form, unchanged
- keeping `fn`, the parameter grammar, and the arrow return means the
  parser reuses the signature grammar and typed HIR reuses signature
  matching: one new AST node kind (requirement), zero new type grammar,
  no second way to declare a method
- an annotated receiver (`self: Self`) rather than a bare `self` keeps
  every parameter explicitly typed. A requirement has no body to infer a
  receiver from, and the receiver type is the single most important fact
  in a requirement, so it is spelled, not implied. A bare `self` would
  special-case one parameter name — today's receiver rule deliberately
  avoids that (`tests/fixtures/fail/invalid_method_receiver.gof`)

### Explicit conformance

```gof
impl Circle: Shape
impl Logger: Render, Serializable
```

Rules:

- `impl Type: Protocol[, Protocol2, ...]` is a block-less top-level
  declaration, newline-terminated; one or more protocols per line
- the conforming type must be a **struct declared in the same module**;
  the protocol may be local or reached through the module graph (imported
  top-level names resolve unqualified, as with functions today)
- conformance is **explicit**: a struct whose methods happen to match a
  protocol does not conform until an `impl` declares it
- `(struct, protocol)` pairs are unique across the module graph; duplicate
  declarations are errors
- the struct's own module is the single conformance authority: an `impl`
  targeting a struct from another module is an error, so two modules can
  never disagree about one conformance

Why `impl Circle: Shape` is the gof-idiomatic explicit form:

- gof already separates where a type's data is declared (the
  `struct Circle:` block) from where its behavior is declared (top-level
  `fn Circle.method(...)`). Conformance is behavior, so it belongs at top
  level beside the methods, not in the struct header
- the struct-header alternative `struct Circle(radius: int): Shape:` would
  force the conformance check to run against methods declared later in the
  file, make existing types unable to adopt a protocol that ships later,
  and mix the data contract with the behavioral contract in one line
- the Rust shape `impl Shape for Circle:` with method bodies in an
  indented block would create a second method-declaration form — exactly
  the parallel structure AGENTS.md §3 forbids. v1 keeps exactly one way to
  declare a method; `impl` declares conformance and points at methods that
  already exist
- the declaration is block-less because there is nothing to put in a body
  in v1: no default method bodies, no witness customization, no method
  mapping. An empty indented block would force gof to invent empty-block
  or `pass` syntax as pure ceremony. Future extensions can grow an
  optional `:` + block suffix without breaking the block-less form
- `Type: Protocol` reuses the annotation reading (`name: type`): the line
  reads "Circle is declared to be a Shape". The comma list matches gof's
  comma-list convention in parameterized annotations and bounds
- `impl` becomes a new lexer keyword. A repository-wide search finds no
  gof source, stdlib module, example, or fixture using `impl` as an
  identifier, and `protocol` is already reserved, so reservation breaks
  nothing that compiles today

### Compile-time conformance checking

Checking lives in typed HIR, after the module graph loads and before
generic bodies and call sites are checked. The pass builds one
**conformance table** — `(protocol, struct) → {requirement → method
symbol}` — which is the single source of truth for impl checking, generic
body checking, call-site bound checks, and witness-table emission. The
parser stays purely syntactic; the evaluator resolves requirement calls
through the same table.

For every requirement, a method `fn Struct.name(...)` must exist with:

- identical parameter count and pairwise identical parameter types after
  substituting `Self` → the conforming struct type
- identical return type after the same substitution
- exact receiver mutability match: a `mut` requirement needs a `mut`
  declaration and a borrowing requirement needs a borrowing declaration.
  No widening in either direction: a `mut` method is stricter at call
  sites (GOF3131), and a generic body derives its mutability discipline
  from the requirement, so the impl may not disagree with it
- receiver parameter names are irrelevant, as they are to method identity
  today

Checking is order-independent: `impl` may appear before or after the
methods it points at, anywhere in the module.

### Protocol-constrained generic functions

```gof
fn describe[T: Shape](shape: T) -> string:
    return shape.name() + " " + to_string(shape.area())

fn bump_all[T: Counter](mut counter: T) -> unit:
    counter.bump()
```

Rules:

- `fn name[T: Protocol[, Protocol2]](params) -> Ret:` — a square-bracket
  type parameter list before the value parameter list, one or more type
  parameters, each with one or more protocol bounds. Brackets match the
  existing parameterized-type surface (`list[int]`, `Result[T, E]`,
  `channel[int]`); `Name: Bound` matches the annotation reading
- every type parameter must carry at least one protocol bound and must
  appear in at least one value parameter annotation, so call-site
  inference is always determined by the signature (unbounded type
  parameters are the Phase 1 generics RFC's decision, not this one)
- type arguments are inferred at call sites by structural extraction from
  argument types (`T` whole, or `T` nested one level inside a
  parameterized builtin annotation — `list[T]`, `dict[T]`, `channel[T]`,
  `task[T]`, `Result[T, E]`, and `Option[T]` from RFC 0049); RFC 0048
  adds the explicit `name[Type](...)` instantiation form and owns its
  disambiguation from indexing
- after inference, each bound is checked against the conformance table;
  only declared conformances satisfy bounds
- inside the body, values of a type parameter may call exactly the
  methods required by that parameter's bounds. Operators, equality,
  indexing, and field access on type-parameter-typed values are rejected
  — protocols guarantee methods, nothing else
- `mut`-receiver requirements are callable only on `mut` generic
  parameters (`mut counter: T` above), which follow RFC 0043's receiver
  convention: an exclusive in-place view for the call duration. At
  instantiation, the argument must be a mutable binding (GOF3131, the
  existing diagnostic)
- a type parameter may be passed to another generic call only when the
  callee's bounds for it are a subset of the caller's bounds
- return types follow today's rules with type parameters in scope:
  explicit annotations act as the contract; inference is unchanged
- generic functions are top-level `fn` only — no generic methods, no
  generic `test fn` or `fixture fn` in v1
- instantiations are ordinary functions for call and spawn purposes:
  `go describe(circle)` is legal and the task value carries the
  substituted return type

### Dispatch contract (ADR 0004)

Conformance is checked once, at compile time, in both profiles —
"protocol conformance" throughout this RFC means the
`(struct, protocol)` relation, never behavioral test conformance.
The profiles differ only in how checked calls lower:

- **Dev profile — witness tables.** Each generic function compiles once,
  type-erased. Each conforming `(struct, protocol)` pair emits one witness
  table: immutable static data with one slot per requirement in declaration
  order, each slot holding the concrete method's code address. Receivers
  are passed by reference (a borrow or inout pointer per the requirement's
  mutability) so an erased body can call any concrete receiver
  representation. A requirement call in the erased body loads the slot and
  makes one indirect call — zero allocation. Witness tables are emitted in
  the module declaring the conformance and instance-cached at module
  boundaries, so monomorphization never runs in the dev profile (ADR 0004)
- **Release profile — monomorphization.** Typed HIR specializes each
  generic function per distinct instantiation; requirement calls become
  direct calls to the concrete methods, inlinable by the SSA pipeline;
  unused instantiations are dead-code eliminated at link
- **Observably identical.** Same conformance errors (all compile-time),
  same results, same panic and error behavior. The only difference is
  call indirection — a performance property, benchmarked, never
  user-visible behavior

Per-value cost is zero in both profiles: conforming structs keep their
plain layout. No vtable pointer, no header, no tag is added to any value
because it conforms; concrete call sites (`circle.area()`) never consult
witness tables.

### v1 scope of conformance

- conforming types are user-declared structs in the same module as the
  `impl`. Enums cannot conform yet because enums cannot declare methods
  today; when enum methods arrive, conformance extends to them with no
  change to this contract
- shipped stdlib types (`Bytes`, streams, `TcpListener`, ...) may conform
  only through stdlib-shipped impls in a future stdlib RFC; user modules
  cannot impl them
- builtins (`int`, `string`, `list`, ...) and owned types (`Box[T]`) do
  not conform in v1
- protocols are flat method sets: no associated types, no generic
  protocols, no protocol inheritance; composition happens at bounds
  (`[T: Shape, Drawable]`)
- requirements are plain `fn` signatures: no async requirements, no
  default bodies, no non-receiver (static) requirements

### AGENTS.md §5 filter

- **who owns the data** — protocols own nothing at runtime; they are
  compile-time contracts. Receivers follow RFC 0043 exactly: `self: Self`
  requirements borrow for reading, `mut self: Self` requirements get
  exclusive in-place access for the call duration. Generic parameters
  follow their concrete taxon's rules under RFC 0043 (value types
  duplicate, owned types move with use-after-move checking, handle types
  share). Witness tables are compiler-owned immutable static data owned by
  the module that declares the conformance
- **where values live** — unchanged by protocols. Conforming structs keep
  their plain layout wherever the program puts them. Generic function
  locals live in the frame: an erased frame with receiver references in
  dev, a specialized frame in release
- **where allocation happens** — nowhere new. Conformance checking and
  bound checks are compile-time; witness tables are static read-only
  data; dev dispatch is one indirect call; release dispatch is a direct
  call. No heap allocation is introduced by any construct in this RFC —
  which is the concrete difference from existentials and the reason they
  are cut
- **where blocking can happen** — nowhere new. A requirement call is an
  ordinary method call; blocking behavior is whatever the concrete method
  does, and it stays explicit and typed (channels, streams, deadlines).
  v1 requirements are plain `fn` signatures, so no requirement can hide
  an await
- **how races are prevented** — protocols add no shared mutable state.
  Witness tables are immutable. `mut` receiver exclusivity follows RFC
  0043: non-handle values cannot alias, so exclusivity holds by
  construction; structs with handle-typed fields conform fine and their
  handle fields stay internally synchronized by the runtime; generic
  functions that spawn tasks with type-parameter payloads follow the
  existing task-boundary transfer/copy rules
- **how it is debugged** — conformance failures are compile-time
  diagnostics with spans, notes, and fix-its. Dev keeps one erased body
  per generic function, so stack traces show the generic frame plus the
  concrete method frame, with few symbols to load (Go-class debug
  surface). Release specializes per instantiation and symbol mangling
  records the instantiated type arguments. There is no runtime
  reflection, no downcast, and no dynamic conformance test: conformance
  is not a runtime question

## Alternatives Considered

### Existential protocol values (protocol-typed values) — cut from v1

The Java / C# / Go interface-value model: `shape: Shape = Circle(1)`,
`shapes: list[Shape]`, protocol-typed parameters, returns, and `match`
scrutinees.

What it buys: heterogeneous collections of mixed conformers
(`list[Shape]` holding `Circle` and `Square`), API boundaries that accept
"anything shaped" without generic ceremony, and the interface model most
programmers already know.

What it costs gof, concretely:

- a protocol-typed value needs a uniform runtime representation — a
  (data, witness) fat value or an inline box with size classes. Value
  taxa have heterogeneous sizes, so coercing `Circle` into a `Shape` slot
  boxes it: **hidden heap allocation at the coercion site**, invisible in
  the source — exactly the cost-model class gof refuses
- dispatch through an existential is dynamic at every call: never
  devirtualizable or inlinable, so "static dispatch" would need an
  asterisk on day one
- ownership: coercion duplicates the payload into the box, and then a
  `mut`-receiver call on an existential needs exclusive access to a box
  that may have been read twice — user-visible aliasing of a *data* value,
  which RFC 0043 deliberately reserves for handles only. Avoiding that
  means forbidding `mut` receivers through existentials or deep-copying
  on read; both are bad contracts
- the open-set use case has a cheap closed-set answer: a gof `enum` over
  the concrete types plus `match` is allocation-free, exhaustive, and
  statically dispatched. The enum is the honest heterogeneous container;
  existentials are the open-set version, and open sets matter mainly when
  the conformer set is genuinely unknown at compile time — rare in
  systems code

Honest loss: v1 cannot express a list of mixed conformers chosen by
callers at runtime, and library authors who want interface-style APIs pay
generic-caller ceremony instead. Generics cover the algorithm-reuse case,
which is the dominant one.

Revisit trigger: a dedicated RFC that fixes the value representation (fat
value versus size-classed inline box), makes any boxing *visible* in the
type (an explicit `Box[Shape]`-style surface would fit gof's explicitness
rules), and settles the `mut`-receiver exclusivity story. Not before
Phase 2 cost data exists.

### Go-style implicit structural conformance

Rejected. Structural matching couples types to protocols by signature
collision: renaming a method silently changes conformance, conformances
are not greppable or diffable, and "who conforms to what" has no single
answer. It also violates the explicitness guardrails. One `impl` line is
the entire ceremony cost of an explicit contract.

### Rust-style `impl` blocks containing the methods

Rejected for v1. Bodies inside `impl` blocks create a second
method-declaration form with different receiver syntax, splitting method
identity across two grammars (AGENTS.md §3: extend good structure, do not
invent parallel structure). Default method bodies are the main thing this
shape would buy; they are deferred independently below.

### Conformance declared in the struct header

Rejected. `struct Circle(radius: int): Shape:` locks conformance to the
type-definition site, forces forward references to methods declared
later, prevents adopting protocols that ship after the type, and mixes
the data and behavior contracts in one line. See the defense of the
top-level `impl` form above.

### Bare receiver `fn area(self) -> int`

Rejected. An untyped receiver special-cases one parameter name and makes
the requirement's most important fact implicit; there is no body to infer
it from. `Self` keeps every parameter annotated, exactly like concrete
receivers.

### Per-value vtable pointers (C++ / Java layout)

Rejected even for release. A vtable slot in every conforming value adds
per-value cost to types whose hot paths never dispatch generically, and
it infects struct layout. ADR 0004's witness tables keep the table out of
the value entirely.

### Default method bodies, associated types, protocol inheritance, generic protocols

Deferred, each for its own reason: default bodies need an override and
conflict story; associated types and generic protocols need the generics
RFC's inference design first; inheritance duplicates what comma-separated
bounds already express. None blocks the v1 use cases.

## Compatibility Impact

- existing programs keep compiling unchanged: no existing construct
  changes meaning, methods keep their declaration form, and concrete call
  resolution never consults protocols
- `protocol` is already a reserved lexer token with no parser rule, so
  programs using it today already fail to parse; activating it cannot
  regress any compiling program
- `impl` becomes a reserved keyword and `Self` becomes a type placeholder
  inside protocol blocks. A repository-wide search finds no gof source,
  stdlib module, example, or fixture using either as an identifier, so
  reservation is verified non-breaking in-repo
- `mut` on protocol-constrained generic parameters extends RFC 0043's
  `mut` receiver convention (exclusive in-place access) from method
  receivers to generic value parameters; it lands with the same typed-HIR
  rule and is the one surface this RFC shares with the ownership slice
- protocol names join the top-level namespace, so the existing top-level
  conflict diagnostic extends to cover them (message update, no new code)
- spec sync obligations for the implementation slice: `spec/language-v1.md`
  moves `protocol` from the v1 surface list into the implemented subset
  with the binding rules above, and its `go`-spawn and plain-call bullets
  gain the instantiated-generic-call forms — the same bullets RFC 0047
  extends for closure targets, so the edits merge; `spec/diagnostics.md`
  gains the four new codes; `roadmap.md` closes the Phase 0 protocol gate
  entry; `docs/site/` and `examples/` gain a protocols walkthrough; the
  VS Code extension grammar, snippets, and tests are updated per
  AGENTS.md §9 with `npm test` and `npm run package` verified

## Diagnostics Impact

All four follow the diagnostics contract (code, severity, primary span,
message, note, optional fix-it), are deterministic, and use only the
codes allocated after RFC 0043's reserved `GOF3129`–`GOF3131`:

- `GOF3132` (error): invalid `protocol` declaration or
  protocol-constrained generic function contract. Covers duplicate
  requirement names, `Self` outside its two legal positions, requirements
  with bodies or missing return types, empty protocol blocks, duplicate
  protocol names across the module graph, unknown types or unknown
  protocols in bounds, type parameters that appear in no value parameter
  annotation, and method calls on type-parameter values that their
  bounds do not guarantee. Notes point at the offending declaration and
  the rule violated
- `GOF3133` (error): invalid conformance declaration. Covers `impl`
  targeting an unknown type, a non-struct type, a struct declared in
  another module, an unknown protocol, duplicates within one `impl`
  list, and `(struct, protocol)` pairs already declared elsewhere. The
  note explains the same-module authority rule
- `GOF3134` (error): a type does not satisfy a protocol. Two contexts:
  at the `impl` (a required method is missing, or parameter types, return
  type, or receiver mutability mismatch after `Self` substitution) and at
  a generic call site (the inferred type argument has no declared
  conformance for a bound). The primary span is the `impl` line or the
  call; notes point at the protocol declaration, the requirement, and —
  when a method exists but mismatches — the method declaration. A
  fix-it offers the missing method skeleton when the method is entirely
  absent
- `GOF3135` (error): protocol name used as a value type. Covers protocol
  names in local bindings, fields, parameters, returns, container element
  annotations, `Result` payloads, and `match` scrutinees. The message
  states that v1 conformance is generics-only and shows the
  `[T: Protocol]` rewrite

Every other error the feature can produce reuses existing codes: `Self`
outside a protocol block resolves as an unknown annotation (GOF3012),
top-level name conflicts extend GOF3021, duplicate methods stay GOF3034,
and `mut`-receiver discipline on instantiated calls stays GOF3131.
`spec/diagnostics.md` gains the four codes when the typed-HIR pass lands.

## Test Plan

- parser / AST tests: protocol blocks, requirements with `Self` and
  `mut` receivers, `impl` lines with single and comma lists, generic
  function headers with bracket bounds; rejection fixtures for bodies on
  requirements, trailing colons, and non-`fn` content in protocol blocks
- typed-HIR conformance-pass unit tests: the full exact-match table —
  parameter types, return types, `Self` substitution, receiver mutability
  both directions; missing methods; duplicate conformance; foreign
  struct targets; order-independence (impl before and after methods,
  protocol declared after use)
- generic body checking: only bounded methods callable on
  type-parameter values; operators, equality, indexing, and field access
  rejected; `mut`-receiver requirements require `mut` parameters; bound
  subsumption for generic-to-generic calls
- call-site inference and checking: conforming arguments accepted;
  structurally matching but non-`impl`'d arguments rejected with GOF3134;
  nested extraction (`list[T]`, `Result[T, E]`); `go` spawn of
  instantiated generic calls
- negative fixtures for each of GOF3132, GOF3133, GOF3134, GOF3135, one
  per listed trigger, with expected spans and notes pinned
- cross-module fixtures: protocol declared in one module, conformance and
  generic use in another; imported-protocol name resolution; duplicate
  protocol names across the module graph
- evaluator behavioral conformance: requirement calls resolve through the
  conformance table with observable behavior identical before and after
  the static checks land
- dev/release dispatch equivalence (native gate, Phase 2): the same
  protocol-heavy program run under both profiles produces identical
  observable behavior
- VS Code extension (AGENTS.md §9): grammar and snippets for `protocol`,
  `impl`, and bracket bounds; `npm test` and `npm run package` verified
- doctest and `examples/` sync: a protocols walkthrough example runs
  under `gof test --docs`

## Benchmark Impact

Per ADR 0004, gates are benchmark-pinned, not asserted:

- dev compile budget: emitting witness tables must not move dev
  wall-clock on a protocol-heavy module set outside the pinned rebuild
  budget; witness tables and their instance caching must keep
  monomorphization out of dev builds entirely
- dispatch cost: one indirect call per requirement call in dev, measured
  as the witness-versus-direct overhead on a protocol-heavy workload;
  release monomorphized calls must be indistinguishable from hand-written
  per-type function calls (the zero-cost claim this RFC makes)
- per-value cost: struct layout with and without conformances is
  byte-identical (a codegen assertion), and heap allocation during
  generic dispatch is zero (assertion plus allocation-stat benchmark)
- binary size: release monomorphization growth is bounded by distinct
  instantiations actually reachable, verified with linker dead-code
  elimination on the scorecard workloads
- a duplication baseline: the end-to-end example compiled with protocols
  versus per-type hand-written functions, to keep the abstraction honest
  in both profiles
