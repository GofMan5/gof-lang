# RFC 0047: Closures

## Summary

Introduce anonymous functions — closures — as `gof`'s first function values:

- expression form `fn (param: Type) -> Ret:` with an indented block body,
  consistent with named `fn` syntax
- captures are by value and follow the RFC 0043 taxa: value types duplicate
  (CoW snapshot at closure creation), handle types share the underlying
  resource, owned types must be written `clone(...)` at the capture site
- no mutable captures: closure bodies may reference only immutable enclosing
  bindings; sharing evolving state goes through channels and handles
- typed-HIR capture lowering: a closure lowers to a capture struct plus a
  function reference; the bootstrap evaluator executes closures directly,
  and the typed-HIR capture checks are the shared static contract
- closures are first-class values: call arguments, container elements,
  return values, `go` spawn targets, and channel payloads; structural type
  `fn(ParamType, ...) -> RetType`
- no self-recursion in anonymous functions in v1

Diagnostics: `GOF3142` through `GOF3145`.

This RFC does not claim named inner functions, first-class references to
named functions, receiver methods on closure values, or any form of mutable
capture.

## Problem

`gof` cannot treat behavior as a value:

- plain calls target only top-level named functions, so every behavioral
  variation is a new top-level declaration. Higher-order helpers (apply,
  adapt, compose) cannot exist, and one-shot logic cannot live next to its
  use site
- RFC 0043 names channels and shared handles as the sanctioned cross-task
  sharing model, but there is no way to package "a worker plus its channel
  wiring" — workers must be top-level functions with every dependency
  passed as an argument
- VISION pins closures as Phase 1 semantic core with explicit capture
  lowering. Phase 2 needs that contract fixed before backend IR calling
  conventions and object layouts are designed
- the evaluator/native split must be decided once: whether the bootstrap
  evaluator materializes capture structs is a parity question, not a
  per-target improvisation

## Proposed Change

### Anonymous function expressions

An anonymous function is an expression:

```gof
fn (param: Type, ...) -> Ret:
    <block>
```

Rules:

- parameters, annotations, and the indented block body follow the named
  `fn` rules; there is no name and no receiver
- `-> Ret` is optional with the same body inference as named functions; a
  body with no `return` statements infers `unit`
- parameters require explicit type annotations: a closure body has no
  call-site context to infer them from, and explicit static types are the
  surface rule
- bodies are indented blocks only — there is no single-line body form
- the `fn` expression may appear in any expression position inside a
  function-shaped body: binding initializers, call arguments, container
  elements, `return` expressions, `match` scrutinees. Module top level
  stays declaration-only
- the parser consumes the closure's block (Indent..Dedent) as part of the
  expression; a separator or closer on the line hosting the Dedent
  continues the enclosing expression. When an in-argument closure reads
  awkwardly, bind it first — style guidance, not semantics

```gof
fn make_adder(n: int) -> fn(int) -> int:
    return fn (x: int) -> int:
        return x + n

fn apply_twice(op: fn(int) -> int, value: int) -> int:
    return op(op(value))

fn main() -> int:
    add5 = make_adder(5)
    return apply_twice(add5, 2)
```

### The closure type

- annotation form `fn(ParamType, ...) -> RetType`: no parameter names, no
  trailing colon, and the return type is mandatory
- usable wherever parameterized builtin annotations are usable: function
  parameters, return types, typed locals, struct fields, `list[...]` and
  `dict[...]` elements, `Result[...]` payloads, channel and task payloads
- compatibility is exact structural match of parameter types and return
  type — no variance and no implicit coercion

### First-class values

- closure values are passed, stored, and returned like any other value;
  the binding, argument, insertion, and return behavior follows the
  closure's taxon (below)
- calling a closure never consumes it: calls borrow (read) the closure
  value. A closure value is consumed only where its taxon moves values
  (owned closures only, per RFC 0043)
- a closure's taxon follows RFC 0043's composite rule over its capture
  set: all captures value/handle makes the closure a value type
  (duplication is O(1) under CoW); any owned capture makes it an owned
  (move-only) value, so use-after-move reports `GOF3129` with the
  existing fix-it
- zero-capture closures duplicate for free: the value is effectively a
  function reference
- closures do not participate in equality or ordering, and they are not
  printable — the same posture as channel and task values
- `go closure(args)` spawns closure calls. The task type carries the
  closure's return type, and the closure value crosses the task boundary
  under the RFC 0043 boundary rule: handle captures stay shared, value
  closures transfer when unique and deep-copy when shared, owned closures
  move. Channel payloads of closure type follow the same rule

**Defer target capture (RFC 0045 interplay).** When defer accepts a
closure value as its target, the defer record must capture the closure
VALUE (per its taxon), not the binding name: `mut c = f; defer c(); c = g`
must call `f` at scope exit. The shipped defer implementation captures
plain-call targets by name, which is safe today because only the
immutable function tables exist; the closures slice must switch defer
target capture to the taxon rules above.
- closure bodies may declare their own locals (immutable and `mut`).
  Mutation of closure-local state is ordinary function behavior; it is
  never visible outside the closure and dies with the call

The handle-sharing pattern this enables:

```gof
fn main():
    results: channel[int] = channel(8)
    square = fn (n: int) -> Result[unit, RuntimeError]:
        return send(results, n * n)

    go square(3)
    go square(4)
    match recv(results):
        Result.Ok(value):
            print(value)
        Result.Err(error):
            print(-1)
```

`square` captures `results` as a handle: the closure and `main` share one
channel, which is exactly the RFC 0043 model for cross-task sharing.

### Capture semantics

The capture set of a closure expression is every free identifier in its
body that resolves lexically to a binding of an enclosing function scope —
including parameters of enclosing functions, loop bindings, and capture
fields of enclosing closures. Top-level names (functions, constructors,
enum variants) resolve as today and are never captures. Module-level value
bindings do not exist. Capture analysis is typed-HIR work: per closure
expression, function-local plus lexical chain, never interprocedural — the
RFC 0043 local-only rule.

Captures initialize when the `fn` expression evaluates. Each evaluation
creates a fresh closure value with fresh captures, so a `fn` expression
inside a loop creates one closure per iteration, capturing that
iteration's bindings.

Per captured binding, read once at closure creation:

| taxon of captured binding | capture field initialization | visible later |
|---|---|---|
| value | duplicate (CoW, O(1)) | never — snapshot at creation |
| handle | share the underlying resource | through the shared resource |
| owned | `clone(binding)` — user-written | never; source never consumed |

**No mutable captures.** A closure body may reference only immutable
enclosing bindings:

- any reference to an enclosing `mut` binding — read, clone, or call —
  reports `GOF3144`
- any assignment (`name = expr`, `mut name = expr`, `name: T = expr`)
  whose target resolves to an enclosing binding reports `GOF3144`;
  closures never implicitly shadow enclosing bindings
- fix-its: snapshot the value before creating the closure
  (`value = mut_binding`), or route updates through a channel or handle —
  the RFC 0043-sanctioned sharing path

```gof
fn main():
    mut base = 10
    # GOF3144: closure bodies reference only immutable bindings
    # add = fn (x: int) -> int:
    #     return x + base
    snapshot = base
    add = fn (x: int) -> int:
        return x + snapshot
    print(add(5))
```

**Owned captures clone at the capture site.** RFC 0043 fixes that reads
never consume and that the compiler never inserts hidden deep copies. A
bare mention of an owned binding must therefore neither move it nor
silently deep-copy it, so the only legal form is an explicit `clone`:

```gof
fn main() -> int:
    box = Box.of(3)                     # owned taxon (RFC 0043; ships separately)
    get = fn () -> int:
        return Box.value(clone(box))    # clone at the capture site
    return get() + Box.value(box)       # `box` was never consumed
```

- a free reference to an owned binding is legal only as the direct operand
  of `clone(...)`. That `clone(binding)` expression is the capture
  initializer: evaluated exactly once, when the closure is created; every
  evaluation during calls reads the captured clone
- the capture struct holds one field per captured binding, so repeated
  `clone(binding)` sites for the same binding share one capture
- any other owned free reference reports `GOF3145` with a wrap-in-clone
  fix-it
- the capture-initializer rule is scoped to owned bindings. `clone` on a
  free value binding is an ordinary per-call expression, and `clone` on a
  free handle binding is an ordinary expression whose sharing behavior is
  identical to capture

**Nested closures.** An inner closure may reference an outer closure's
parameters and captured names; those are bindings whose taxa decide the
inner capture by the same table. Inner captures initialize when the inner
`fn` expression evaluates — once per outer call. A closure may itself be
captured by another closure under the same rules.

```gof
fn main():
    mut makers: list[fn() -> int] = []
    for i in range(3):
        maker = fn () -> int:
            return i * 10
        makers = append(makers, maker)
    first = makers[0]
    second = makers[1]
    print(first())    # 0
    print(second())   # 10
```

Each iteration's loop binding is a distinct immutable binding, so every
created closure snapshots its own `i`.

**Restrictions inside generic declarations.** A closure created inside a
generic function or method may not capture values of a type-parameter
type, and closure-type annotations in generic declarations — signatures,
struct fields, enum payloads — may not mention the declaration's type
parameters. The taxon of a `T`-typed capture is
instantiation-dependent, so per-capture taxon classification is
undecidable in the generic body; a per-`T` capture-struct layout would
force dev-profile monomorphization, violating ADR 0004's
one-erased-body invariant; and protocol witness tables carry method slots
only — no duplication or destruction entries. Workarounds are explicit:
hoist the value into a concrete type, or route it through a channel or
handle. The restriction lifts only through an RFC that extends witness
tables with taxon operations and specifies the erased capture
representation.

**No self-recursion in v1.** An anonymous function has no name, and the
binding being initialized is not in scope inside its own initializer, so
the body cannot reference the closure it defines (existing `GOF3002`).
Mutual references between sibling closures fail for the same reason: the
later binding is not yet initialized at the earlier closure's creation.

**Control flow inside closures:**

- `return` binds to the innermost enclosing function body — the closure
- `break` and `continue` target loops inside the closure body only; there
  is no non-local escape through a closure, so references to an enclosing
  loop report the existing outside-a-loop codes (`GOF3053`/`GOF3054`)
- `expr?` propagates against the closure's own return contract
  (`GOF3075` otherwise)
- a panic inside a closure behaves like a panic inside a named function;
  inside a spawned closure it surfaces at the join as
  `RuntimeError.TaskPanicked` under the existing task rule
- unsafe context is lexical and function-local (RFC 0046): a closure body
  never inherits an `unsafe:` block enclosing the `fn` expression, and a
  closure body may not open its own `unsafe:` block in v1 — unsafe work
  needed inside a closure goes through a named wrapper function

### Typed-HIR capture lowering

Closures are typed-HIR/MIR semantics from day one (VISION), not an
evaluator-only convenience:

1. **Capture analysis (typed HIR).** For each closure expression, resolve
   free identifiers through the lexical chain, classify each captured
   binding by RFC 0043 taxon, enforce the immutability rule (`GOF3144`)
   and the clone-at-capture rule (`GOF3145`), and produce an explicit
   capture list `[(name, type, initializer)]` in source order of first
   free reference — deterministic spans, deterministic diagnostics.
2. **Synthetic function extraction.** The closure body lowers to a
   module-unique synthetic function
   `__closure_<module>_<n>(captures: CaptureN, params...) -> Ret`. The
   capture list becomes a flat capture struct type `CaptureN` with one
   field per capture. Synthetic names cannot collide with user names and
   are not referenceable from source.
3. **Creation.** `MakeClosure(fn_ref, capture_inits...)` allocates and
   initializes the capture struct — the single allocation closure
   creation performs. Zero-capture structs may elide to a bare function
   reference as an optimization with unchanged observable behavior.
4. **Call.** `CallClosure(closure, args...)` loads the function reference
   and passes the capture struct plus the declared arguments. Callee
   type, arity, and argument compatibility are typed-HIR checks
   (`GOF3143`).

MIR shape: a closure value is a pair (function reference, capture-struct
reference). Value-taxon closures duplicate by sharing the capture struct
under CoW; owned closures are move-only. Invariants: capture order is
source-deterministic; the capture struct is flat (no environment chains);
capture structs are immutable after creation; each captured name binds
exactly once in the synthetic function's capture-plus-parameter scope.

**Evaluator.** The bootstrap evaluator executes closures directly: a
runtime closure value holds the parameter list, the body, and a
captured-bindings map, and calls evaluate the body with parameters and
captured names bound. It does not materialize capture structs — that is
the native lowering's business. The typed-HIR capture analysis is the
shared static contract: both targets enforce identical `GOF3144` and
`GOF3145` rules, mirroring RFC 0043's move-checker posture. Escape
analysis and stack promotion of capture structs are backend optimizations,
orthogonal to semantics.

### AGENTS.md §5 answers

| question | answer |
|---|---|
| who owns the data | the closure value owns its capture struct; field taxa follow RFC 0043 (value CoW, owned move-only, handle shared) |
| where values live | a flat capture struct allocated at closure creation; the backend may stack-promote non-escaping ones |
| where allocation happens | exactly one allocation per closure creation, at the `fn` expression site; zero-capture closures may be static |
| where blocking can happen | never in creation or capture; only inside captured-handle operations, which carry their own existing blocking and cancellation contracts |
| how races are prevented | closure bodies cannot write enclosing state (no mutable captures); the only shared state reachable from a closure is handle-typed and internally synchronized; closure values crossing task boundaries follow the RFC 0043 boundary rule |
| how the feature is debugged | deterministic capture list with spans on the closure expression; `GOF3142`–`GOF3145` notes point at the capture site; existing test, doctest, and fixture surfaces run closures unchanged |

## Alternatives Considered

### Mutable captures (`mut` capture lists or by-reference capture)

Rejected. This is the Python/JavaScript/Swift `var` model, and it
reintroduces exactly what the language refuses: implicit shared mutable
state as the default model (AGENTS.md §5), user-visible aliasing of plain
data values, which RFC 0043 permits only for handles, and unsynchronized
data races as soon as two tasks hold closures over the same `mut`
binding. Accumulation-style code stays expressible through returned
values, higher-order helpers built on closures, and channels — the
sanctioned sharing path that this RFC's handle captures make first-class.

### Move-on-capture for owned types

Rejected. RFC 0043 fixes that reads never consume; a bare mention of an
owned binding inside a closure body would silently kill the enclosing
binding, and the common "mention it and keep using it" pattern would
become a two-binding dance. Clone-at-capture-site keeps the deep copy
visible exactly where the value flows into the closure and never consumes
the source.

### Rust-style explicit capture lists

Rejected for v1. Capture lists defend against accidental mutability and
accidental expensive captures — both forbidden outright here with
dedicated diagnostics. The list would be ceremony without added safety.
Revisit only if real capture sets prove hard to reason about.

### Arrow or `lambda` syntax

Rejected. A second function syntax buys nothing: one `fn` form keeps one
mental model, and surface-area growth is the lowest priority in the
priority order.

### Implicit named-function-to-closure coercion

Deferred. Referencing a named function as a value would be an implicit
conversion, which the language rejects. The explicit zero-capture adapter
(`fn (x: int) -> int: return name(x)`) is one block and costs nothing; a
future RFC may introduce named function references as their own typed
value.

### Named inner functions

Deferred as the v1 recursion story. Nested named declarations would lift
the no-self-recursion restriction and enable mutual recursion inside
function scope, but they add declaration position, shadowing, and
visibility rules on top of the same capture machinery. Recursion today
goes through top-level named functions. The capture contract in this RFC
is designed so named inner functions, if accepted later, reuse it
unchanged.

### GC-managed linked environments

Rejected. Classic environment chains hide allocation, defeat flat-object
optimizations, and are hostile to the serialized IR cache boundary. The
capture struct is flat, deterministic, and CoW-friendly.

## Compatibility Impact

- purely additive: `fn (...)` in expression position is a parse error
  today, and `fn(...)` in annotation position is `GOF3012` (unknown
  builtin type annotation) today, so no valid program changes meaning
- `go` extends its target rule from top-level named function calls to
  closure values as well — the spec's `go` contract bullet gains the
  closure-target form, and `GOF3008`'s scope grows accordingly; the
  implementation slice syncs `spec/language-v1.md` for this change
- one existing diagnostic grows scope: `GOF3008` (invalid `go` spawn
  target) extends from named-function-only targets to closure targets;
  all other existing diagnostics keep their scope, and named-function
  call errors stay on their existing codes
- owned-capture rules apply when RFC 0043's owned types ship; nothing
  shipped can violate them yet — the rule is fixed now so the first owned
  type lands inside this contract
- `gof test`, doctests, and fixtures are unchanged; `test fn`,
  `fixture(scope) fn`, and method bodies may create closures under the
  same capture rules
- sync obligations when implemented (AGENTS.md §8):
  `spec/language-v1.md` surface, `spec/diagnostics.md` codes, `docs/site`,
  `examples/`, `roadmap.md`, and the VS Code extension grammar and
  snippets (AGENTS.md §9)

## Diagnostics Impact

All four follow the diagnostics contract (code/severity/span/message/
note/fix-it):

- `GOF3142` (error): invalid anonymous function contract — duplicate
  parameter names, missing parameter type annotations, receiver-style
  `self` parameters, a return annotation incompatible with the body's
  return paths, or a missing return annotation the body cannot infer
- `GOF3143` (error): invalid closure call — a callee expression that does
  not resolve to a closure type, wrong argument count, or incompatible
  argument types at a closure call
- `GOF3144` (error): mutable capture not supported — a reference to an
  enclosing `mut` binding or an assignment targeting an enclosing binding
  inside a closure body; note explains capture-by-value at creation;
  fix-it suggests a snapshot binding before creation or a channel/handle
  route
- `GOF3145` (error): owned capture without an explicit clone — a free
  reference to an owned binding not written as `clone(binding)`; note
  explains the clone-once-at-creation semantics; fix-it wraps the
  reference in `clone(...)`

`spec/diagnostics.md` gains all four codes when the typed-HIR pass lands.

## Test Plan

- parser/AST: `fn` expressions in every permitted position, nested
  closures, zero-parameter forms, block-body indentation errors
- shared parser regression with RFC 0048: `fns[0](arg)` parses as
  indexing; `identity[int](41)` parses as type application; a bracket
  identifier naming both a type and an in-scope value takes the
  type-list parse with a rename hint for the value use
- typed HIR: capture-analysis positives and negatives for
  `GOF3142`–`GOF3145`; taxon classification per RFC 0043; per-iteration
  loop captures; nested-closure capture chains; closure-in-closure
  capture; no-self-recursion (`GOF3002`)
- typechecking: structural `fn(...)` compatibility, exact-match
  rejection, closure-typed annotations in every annotation position
- evaluator: snapshot semantics at creation; clone-at-capture evaluated
  once; handle sharing; first-class flows (arguments, containers,
  returns, `go`, channel payloads); `return`, `break`/`continue`, and
  `expr?` scoping; panic propagation from spawned closures
- conformance: identical observable behavior under the evaluator before
  and after the static checks land; one negative fixture per new code
- examples: a closures walkthrough (higher-order helper, channel-sharing
  worker, snapshot semantics)
- VS Code extension (AGENTS.md §9): grammar and snippets for `fn`
  expressions and the `fn(...)` type; `npm test` and `npm run package`
- docs/site: `npm run build` in `docs/site/`

## Benchmark Impact

Phase 2 gates, not bootstrap claims (RFC 0043 posture):

- closure creation cost: one allocation per creation, plus the
  zero-capture elision rate
- call overhead: `CallClosure` versus a direct named call, and the
  devirtualization rate when the callee is statically known
- value-closure duplication cost: a reference count bump with no field
  copies
- escape-analysis promotion rate: share of capture structs that stay on
  the stack
- clone-at-capture cost is user-visible by design; there is no hidden
  copy to benchmark
- no zero-cost claim for closures is accepted without a benchmark
