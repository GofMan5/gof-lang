# RFC 0054: Const Expressions

## Summary

- top-level `const name: Type = expr` declarations introduce named compile-time
  constants; `const` becomes a new reserved keyword; the type annotation is
  mandatory
- initializers come from a restricted const-expression grammar: literals,
  arithmetic/comparison/logical operators, enum/struct/`Result` construction of
  const-evaluable values, references to other consts, and exactly one builtin
  call — `len` over const list, string, or dict values
- values are evaluated once, at the declaration, in a typed-HIR const pass;
  every failure is a GOF-coded compile diagnostic (`GOF3173`–`GOF3177`), never
  a runtime diagnostic
- const types are deeply value-taxon (RFC 0043): no handles, no owned types,
  no channels, no `mut` members anywhere in the type
- consts are usable in every expression position that accepts an immutable
  binding of the same type, and the const-expression grammar is specified
  independently of the declaration form so a future RFC can reuse it for enum
  payload defaults
- const functions, general compile-time function execution, and const generics
  are out of scope and rejected in Alternatives

## Problem

`gof` cannot name a compile-time value:

- there are no top-level value bindings at all — top level declares only
  `struct`, `enum`, `fn`, `test fn`, and `fixture(scope) fn`; every named value
  is function-local and initialized at runtime
- shared magic numbers and strings are duplicated at each use site, or hidden
  behind zero-argument `fn` wrappers that are re-evaluated on every call, are
  not compile-time values, and cannot seed any position that will later
  require a compile-time-known value
- inline expressions carry no intent: nothing distinguishes `4096` used as a
  buffer size from `4096` used as a port, and nothing documents that a value
  is fixed before the program runs
- VISION.md Phase 1 lists const expressions inside the semantic core, and the
  features that may follow it (enum payload defaults, for example) each need
  one fixed const-expression grammar to build on instead of inventing their
  own

The gap is a Phase 1 exit item, not a convenience. The language promises
explicit, predictable semantics, and "this value is fixed before the program
runs" is currently inexpressible.

## Proposed Change

### Syntax

A new top-level declaration form:

```gof
const MAX_CONNECTIONS: int = 64
const GREETING: string = "hello, " + "world"
const DEFAULT_LIMITS: list[int] = [10, 20, 50]
const TABLE_SIZE: int = len(DEFAULT_LIMITS)
const EMPTY_GATE: Result[int, RuntimeError] = Result.Err(RuntimeError.NetClosed)
```

- `const name: Type = expr` — the `const` keyword, an identifier, a mandatory
  type annotation, a mandatory initializer, newline-terminated; one declaration
  per line, no indented block, same shape class as `import`
- top level only. A `const` inside a function body is a parse error through
  the existing token diagnostics (`GOF2001`/`GOF2002`); no new parse code is
  added
- the mandatory annotation pins the public cross-module contract at the
  declaration, mirrors the explicit-return-type discipline of fixtures
  (RFC 0041), and keeps type eligibility and evaluation independent of
  inference order between modules
- identifiers follow the normal rules; `UPPER_SNAKE_CASE` is the recommended
  (documented, not compiler-enforced) spelling
- consts are immutable by construction; there is no `mut` variant, and consts
  obey exactly the immutable-binding discipline: a `mut`-receiver method call
  targeting a const is rejected exactly as it is for any immutable binding
  (`GOF3131`, unchanged)

`const` becomes a reserved lexer keyword. A repository-wide search for
`const` across all 182 `*.gof` files — `examples/` (65), `stdlib/` (6),
`tests/fixtures/` (110), and the VS Code extension's test fixture — returns
zero matches: not as an identifier, and not even inside comments or strings,
so the reservation breaks no shipped source. This mirrors how RFC 0044
reserved `impl` and RFC 0046 formalized the already reserved `unsafe`.

### The const-expression grammar

The parser accepts the initializer as an ordinary expression; the const pass
validates the typed tree against this restricted subset. There is no parallel
parser mode and no parser-level typing (AGENTS.md §4):

```
const_expr := INT_LITERAL
            | STRING_LITERAL
            | "true" | "false"
            | "[" [ const_expr { "," const_expr } ] "]"        # list literal, homogeneous
            | "{" [ STRING ":" const_expr { "," STRING ":" const_expr } ] "}"  # dict literal
            | "-" const_expr                                   # int negation
            | "not" const_expr                                 # bool
            | const_expr ("+"|"-"|"*"|"/"|"%") const_expr      # int arithmetic
            | const_expr "+" const_expr                        # string concatenation
            | const_expr ("=="|"!="|"<"|"<="|">"|">=") const_expr  # existing equality/ordering domains
            | const_expr ("and"|"or") const_expr               # bool, short-circuit
            | IDENT                                            # reference to another const
            | IDENT "." VARIANT                                # unit enum variant
            | IDENT "." VARIANT "(" [ const_expr { "," const_expr } ] ")"  # payload enum
            | IDENT "(" [ const_expr { "," const_expr } ] ")"  # struct construction, positional
            | "Result" "." ("Ok"|"Err") "(" const_expr ")"
            | "len" "(" const_expr ")"                         # list, string, or dict operand
```

Rules:

- precedence, associativity, and every operator's operand domains are exactly
  the existing expression rules; the const grammar is a subset, not a variant
- comparison and equality follow today's domains: ordering on `int` and
  `string`, equality wherever the spec already grants explicit equality
  semantics (`list`, `dict`, same-type struct/enum, compatible `Result`) —
  const evaluation invents no new operator behavior
- `Option.Some`/`Option.None` and tuple expressions join the grammar when
  RFC 0049 and RFC 0050 land, through their already-specified constructor
  forms
- excluded forms are rejected with `GOF3174`: function and method calls,
  indexing, field access, closures, postfix `?`, `match`, `send`/`recv`, and
  any identifier that does not resolve to a const

### Const-eligible types: value taxa only

The declared type (and every member recursively within it) must be deeply
value-taxon per RFC 0043:

- eligible: `int`, `string`, `bool`, `list[T]`, `dict[T]` with eligible `T`,
  user enums and structs whose payload/field types are eligible,
  `Result[T, E]` with eligible `T`/`E` (and `Option[T]`, tuples, per their
  RFCs)
- `unit` and `json` are value taxa but have no constructor in the const
  grammar, so they are eligible yet unrepresentable; nothing is lost
- `Bytes`, `SocketAddr`, and `NetDeadline` are value taxa (immutable
  snapshots) but are only constructible through runtime helpers, so no const
  expression can produce them today; admitting them is future work if a pure
  constructor is ever specified

Excluded taxa and why:

- **handle types** (`channel`, `task`, `cancel_token`, streams,
  `TcpListener`, `TestContext`, `TempDir`, `TempFile`): a handle is a
  reference to a runtime resource with state and a final-reference-close
  lifetime (RFC 0043). A const handle would be a resource created before the
  program runs, aliased by every use site, and never deterministically
  closed — exactly the hidden global state AGENTS.md §5 forbids, plus an
  effectful construction (opening, registering, arming timers) smuggled into
  compile time
- **owned types** (`Box[T]`, and any composite carrying an owned member —
  ownership is contagious per RFC 0043): owned values are move-only, but a
  const must be usable at every use site, any number of times, exactly like an
  immutable binding; the first use would consume it. A compile-time `Box`
  would also be a heap allocation with no owner, and move-checking a globally
  shared value would require interprocedural analysis, which the local-only
  analysis rule of RFC 0043 rejects
- **channels**: handle taxon, covered above

The deep check also rejects any type with a `mut` field anywhere (none exist
today); when a future RFC adds `mut` struct fields, such structs are excluded
so a const can never become shared mutable state.

### Evaluation: typed HIR, once, at the declaration

A new const pass runs in typed HIR, after the initializer has been
typechecked by the ordinary pass (existing codes unchanged — payload arity,
unknown variant, unknown field, and operand-domain errors all fire exactly as
today). The const pass then:

1. validates the declaration contract — name uniqueness and acyclicity
   (`GOF3173`)
2. validates the typed initializer against the const grammar (`GOF3174`)
3. checks deep value-taxon eligibility of the declared type (`GOF3175`)
4. checks the initializer's type against the declared annotation (`GOF3176`)
5. evaluates the initializer and memoizes the value into the module's const
   table, keyed `(module, name) -> (type, value)` (`GOF3177` on evaluation
   fault)

Evaluation semantics:

- each const is evaluated exactly once per compiled module, at its
  declaration; downstream layers and the runtime read the memoized table and
  never re-evaluate
- operator semantics are the runtime's semantics, reusing the interpreter's
  value-level operator implementations through a pure entry point so const and
  runtime arithmetic cannot drift; the one fault class promoted to compile
  time is division or modulo by a zero divisor (`GOF3177`), the compile-time
  counterpart of the runtime `GOF3068` class — a declared const can never
  raise a runtime diagnostic

  **Implementation phasing note (v1):** the typed pass detects the literal
  zero divisor (the common case) and reports GOF3177 at compile time. A
  const-referenced zero divisor (`const Z: int = 0` then `10 / Z`) cannot
  be promoted through the typed layer in v1: the one-way layer discipline
  (ADR 0002) keeps the typed pass above the evaluator, and const values
  only materialize in the module's const evaluation. Such programs fault
  during const initialization — before the first statement of any
  function runs — so the observable contract "a declared const never
  raises a runtime diagnostic mid-execution" still holds. Full
  value-level promotion joins the strict per-module views re-entry
  (Phase 2), where const evaluation moves to a typed-layer pure
  evaluator shared with the native backend.
- `and`/`or` keep left-to-right short-circuit; evaluation is deterministic,
  environment-independent, effect-free, and total
- termination holds by construction: the grammar contains no calls, no loops,
  and no self-reference; const-to-const references are the only recursion
  vector and cycles are rejected (`GOF3173`)
- declaration order within a module does not constrain references; evaluation
  follows the const dependency DAG, and cross-module references follow the
  module import DAG (import cycles are already rejected), so imports evaluate
  before importers

The bootstrap-native bundle recompiles at launch, so its const pass re-runs
once per launch; because evaluation is deterministic and effect-free,
observable behavior is identical.

### Name resolution

- const names join the existing top-level namespace rules: a const conflicts
  with another const, a function, a struct, an enum, a fixture, an import
  alias, or a reserved stdlib module name (`bytes`, `io`, `time`, `net`,
  `http`, `testing`) — locally or across the module graph — and is rejected
  with `GOF3173`
- a const may not take a builtin helper name (functions may shadow builtins
  today because call resolution is single-meaning; a const would give one
  token a value meaning and a call meaning simultaneously) — also `GOF3173`
- after `import`, consts are referenced by unqualified name, exactly like
  imported functions, structs, and enums

### Usage positions

- every expression position that accepts an immutable binding of the same
  type: binding initializers, call arguments, operator operands, conditions,
  index expressions, `match` scrutinees, `send`/`go` payloads, `return`
  expressions, `assert` arguments, fixture and test bodies, doctest snippets
- const values cross task boundaries under the RFC 0043 rules unchanged:
  value payloads transfer when unique or deep-copy deterministically when
  shared
- not usage positions in v1: type annotations (no const generics — type
  positions take types only) and `match` arm patterns (const patterns do not
  exist); both stay out until their own RFCs
- the const-expression grammar is deliberately specified as a standalone
  grammar so a future enum-payload-defaults RFC can reuse it verbatim; this
  RFC adds no payload-default syntax

### Storage and the AGENTS.md §5 guardrails

| question | answer |
|---|---|
| who owns the data | the module owns one canonical value per `(module, name)`; every use site receives a value-semantic duplicate per RFC 0043 (Copy + CoW), so no use site aliases the canonical value |
| where values live | typed-HIR const table at compile time; bootstrap evaluator: materialized module table; native target (Phase 2): read-only static data, lowered and conformance-tested rather than assumed |
| where allocation happens | compile time, once per const, bounded by initializer size; reads allocate nothing; first divergent mutation materializes a CoW copy |
| where blocking can happen | nowhere — the grammar admits no blocking construct and evaluation terminates by construction |
| how races are prevented | consts are deeply immutable values; reads never mutate; mutation paths duplicate first; there is no user-visible aliasing outside handle taxa, which consts exclude |
| how the feature is debugged | declaration-site diagnostics with spans and notes; const values render through the existing `print`/`to_string`/snapshot surfaces; no new runtime failure modes exist |

### Pipeline placement

- lexer: `const` keyword token
- CST/AST: `ConstDecl` top-level declaration node; initializer parsed with the
  ordinary expression grammar
- HIR: `ConstDecl` with name, declared type, and initializer expression
- typed HIR: the const pass described above; the const table is an explicit
  typed-HIR artifact with documented invariants
- MIR/SSA/backend: const uses resolve to table entries and lower to static
  data references; the evaluator materializes values from the table

One-way dependencies are preserved; each layer has direct tests.

## Alternatives Considered

### Const functions / general compile-time function execution

Rejected for v1. Arbitrary `const fn` needs termination checking, an
effect-free call graph analysis, deep evaluator surface, and a much larger
diagnostics contract. The restricted grammar is total by construction — no
calls, no loops, no recursion — so correctness is structural, evaluation
cost is bounded, and the door stays open: a future CTFE RFC would extend this
same const table and reuse this grammar as its terminal case.

### Const generics / fn-level const generics

Rejected as out of scope. Type-parameterized constants (`list[int, N]`,
`fn f[N: const int]`) require an interaction design between the type system
and the const table that deserves its own RFC after generics (RFC 0048) and
this RFC settle. This RFC deliberately adds no type-position usage, so the
future design starts from a stable grammar and a stable evaluation contract.

### Local `const` inside function bodies

Rejected. Local immutable bindings already exist (`name: type = expr`), so a
local `const` adds grammar without semantics; the compiler does not
constant-fold locals today, and "initializer is compile-time-known" as a local
guarantee is a lint, not a language contract. Top level is where the gap is.

### Keyword-less top-level bindings or `let`

Rejected. A bare top-level `name: type = expr` (or a `let` keyword) would
either be silently runtime-initialized — a hidden second initialization model
for module-level values — or silently compile-time, which is exactly the
hidden magic AGENTS.md §5 forbids. An explicit `const` keyword makes the
compile-time contract visible at the declaration site and matches the
VISION.md Phase 1 vocabulary. `const` also matches the reserved-keyword
precedent (`impl`, `unsafe`) with a verified zero-breakage reservation.

### Type-inferred consts (`const name = expr`)

Rejected. A const is a cross-module public contract; leaving its type to
inference couples every importer to the initializer's shape and to inference
order between modules. Fixing the type at the declaration is what makes
taxon eligibility, evaluation, and downstream IR lowering all
declaration-local.

### A broader const builtin set

Rejected. Every builtin admitted into const evaluation must be proven total
(no `Result`-returning partial helpers), deterministic, and
environment-independent. `len` is the single builtin that satisfies all three
and answers a recurring need — derived sizes of const data
(`const TABLE_SIZE: int = len(TABLE)`) — because its result is a fixed
property of the literal structure itself. `first`, `slice`, `min`,
`parse_int`, and friends are partial and `Result`-returning, which would
smuggle recoverable errors into compile time; `env`, `argv`, `cwd`,
`unix_millis`, and I/O helpers would make compilation depend on the host;
blocking builtins (`sleep`, `send`, `recv`) are meaningless at compile time.
The closed set is one builtin, and additions require their own proof.

### Enum payload defaults in this RFC

Rejected as premature. Defaults interact with exhaustiveness, construction
arity, and struct/enum defaults as one design. This RFC only guarantees the
grammar is specified independently so that future RFC inherits it instead of
redefining it.

## Compatibility Impact

- reserving `const` breaks no shipped source: the repository-wide `*.gof`
  search found zero occurrences (see Syntax)
- no existing program changes meaning: there were no top-level value
  bindings, no expression grammar changes, and consts only add a new
  name-resolution source; existing diagnostics keep their codes, spans, and
  meanings
- the initializer is typechecked by the ordinary pass first, so today's
  operand, arity, and variant diagnostics fire unchanged inside const
  declarations
- tooling surfaces: `tools/vscode-gof` grammar gains the `const` keyword
  (declaration group) plus snippet and test updates, verified with `npm test`
  and `npm run package` (AGENTS.md §9); `gof fmt` gains the single-line
  `const name: Type = expr` rule; whole-file doctest examples may start with
  a `const` declaration, since they already accept top-level declarations
- stdlib modules may adopt consts later as a teaching follow-up; no stdlib
  change is required or made by this RFC
- no runtime, ABI, CLI, or packaging behavior changes for existing programs

## Diagnostics Impact

All five follow the diagnostics contract (code/severity/span/message/note/
fix-it). The highest code reserved in-tree is `GOF3157` (RFC 0050);
`GOF3158`–`GOF3172` stay reserved for sibling RFCs in flight. This RFC
allocates `GOF3173`–`GOF3177` and nothing else:

- `GOF3173` (error): invalid `const` declaration contract — duplicate const
  name, top-level conflict with a function, struct, enum, fixture, import
  alias, or reserved stdlib module name (locally or across the module graph),
  a const name colliding with a builtin helper name, or a const reference
  cycle. Primary span: the const name. Note: the conflicting declaration or
  the cycle edge. No fix-it.
- `GOF3174` (error): const initializer is not a const expression — a
  subexpression outside the grammar: function or method call, unsupported
  builtin call, indexing, field access, closure, postfix `?`, or an
  identifier that does not resolve to a const. Primary span: the offending
  subexpression. Note: the allowed grammar summary. No fix-it.
- `GOF3175` (error): const type is not const-eligible — the declared type,
  or any member anywhere within it, is a handle type, an owned type, or
  carries a `mut` field. Primary span: the declared type. Note: the offending
  member path and the deeply-value-taxon rule. No fix-it.
- `GOF3176` (error): const initializer does not match the declared type.
  Primary span: the initializer. Note: expected versus evaluated type.
  `GOF3013` keeps its current scope (local annotated bindings, parameters,
  declared return types); no existing code changes meaning. No fix-it.
- `GOF3177` (error): const evaluation fault — evaluation of a well-typed
  initializer failed at compile time; the v1 case is division or modulo by a
  zero divisor. Primary span: the faulting operator. Note: the value is fixed
  at declaration, so the runtime diagnostic class (`GOF3068`) is promoted to
  compile time here. No fix-it.

`spec/diagnostics.md` gains the five codes when the typed-HIR const pass
lands.

## Test Plan

- lexer/parser Rust unit tests: `const` lexes as a keyword; the top-level
  declaration parses; statement-position `const` and malformed shapes reuse
  `GOF2001`/`GOF2002`; formatter renders the single-line form
- typed-HIR const-pass unit tests: one case per `GOF3173`–`GOF3177` subcase;
  memoization (each const evaluated exactly once); dependency-DAG evaluation
  order; cross-module evaluation through imports; cycle detection
- positive fixtures under `tests/fixtures/`: every grammar element —
  literals, all operators including division by a nonzero const, comparisons
  and logicals, enum/struct/`Result` construction, `len` over const
  list/string/dict, const-to-const references, cross-module references
- negative fixtures with exact codes, following the repo's fixture style:
  one file per diagnostic subcase, including the taxon negatives (const
  channel, const `Box[T]`, const struct-with-handle-field) and the
  promoted-fault case (division by a const zero) as the compile-fail
  counterpart of the existing `tests/fixtures/runtime-fail/`
  `division_by_zero.gof`
- evaluator/interpreter tests: const uses return the memoized values; a
  program using consts behaves identically to the equivalent program with
  inlined literals (conformance-oracle equivalence)
- doctests: a whole-file plain ` ```gof ` example teaching consts; examples/
  gains a cross-module const example (importer plus defining module)
- VS Code extension: grammar keyword test plus `npm test` and
  `npm run package`
- docs sync (AGENTS.md §8): `spec/language-v1.md` const bullets,
  `spec/diagnostics.md` codes, `roadmap.md` Phase 1 const-expressions item,
  docs/site content

## Benchmark Impact

No dedicated benchmark gate is introduced. Const evaluation is compile-time
work proportional to the initializer graph, executed once per const per
compiled module and never re-run at use sites; modules that declare no consts
compile identically to today, so incremental compile budgets are unaffected.
In the native target, consts lower to read-only static data and reads become
loads with no allocation; those claims land with the Phase 2 gates as
conformance-tested behavior, not asserted numbers, per the VISION honesty
rules. The bootstrap evaluator gains a measurable but unclaimed win by
replacing per-call recomputation of zero-argument helper functions with table
reads.
