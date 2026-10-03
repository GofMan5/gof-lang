# RFC 0053: Module Visibility

## Summary

Activate the `module` token — already a lexer keyword
(`TokenKind::Module`), already listed in the spec v1 surface, and dead
today — as an explicit export declaration:

- a `module:` block, indented bare export entries, one per line, as the
  **first** top-level declaration of a file, listing exactly the
  top-level names that module exports
- visibility is **default-private and file-scoped**: without a `module:`
  block, a module's declarations are not importable; the export list is
  the only way in
- an import contributes **exactly the imported module's own exported
  names** to the importer's namespace — never the module's imports, so
  no transitive re-export, no leakage
- granularity is **top-level declarations only** (structs, enums,
  functions, protocols); fields and methods are not separately visible
  and ride with their type's export
- protocol conformances (RFC 0044) are visible wherever **both** the
  conforming struct and the protocol are visible
- compatibility: a grandfather window keeps no-`module:` files
  default-public (own declarations only) until the v1 freeze, then the
  default flips to private; a `gof mod export` codemod and a window-only
  warning (GOF3172) carry the migration

This RFC is the Phase 1 "module visibility" item named in VISION.md's
phase table. It changes the name-availability contract of the existing
flat import namespace; it does not change import resolution, import
cycles, package manifests, lockfiles, or the reserved stdlib names.

## Problem

Today every declaration in every loaded module is public, and the module
graph is one flat namespace. `ModuleResolver::collect_items`
(`compiler/gof-compiler/src/module_graph.rs`) walks `load_order` and
merges the structs, enums, and functions of **every transitively loaded
module** into one list with no per-module visibility. Three concrete
consequences, all verifiable in the repo today:

- **transitive leakage.** In the `loads_functions_from_local_package_dependencies`
  test, `app/src/main.gof` imports `package_math`, whose `lib.gof`
  imports `ops`. `multiply` from `ops.gof` — a module `main` never
  imported — lands in `main`'s namespace and is nameable from `main`.
  The import closure, not the import list, defines what a module can
  see.
- **no internal surface.** A helper in `geometry.gof` is indistinguishable
  from `geometry`'s API. Every top-level name is a public commitment;
  renaming an internal helper can break any importer anywhere in the
  graph. The duplicate-name rules (GOF3016, GOF3020, GOF3021, GOF3029)
  enforce this globally: two modules cannot even privately share a
  helper name.
- **hidden global behavior.** Per the spec, a top-level user function
  shadows a same-named builtin call. Because the graph is one namespace,
  a private-looking helper named `print` in `geometry.gof` silently
  changes what `print(...)` means in `main.gof`. That is hidden global
  state of exactly the kind AGENTS.md §5 forbids.

The contract the language actually wants already exists — but only in
prose. `spec/language-v1.md` hand-maintains per-module exposure lists
("the shipped `io` module currently exposes `ReadStream`,
`open_read_stream`, ..."). The spec is the export list; the language has
no way to state it. Meanwhile `module` sits in the lexer as a reserved
keyword with no parser rule — a promise with no contract, the same state
`protocol` was in before RFC 0044.

Pre-v1 is the last cheap moment to fix this. VISION.md lists module
visibility inside Phase 1, whose exit criterion is "core frozen".
Freezing a public-everything v1 would lock the flat-closure namespace in
permanently; every later visibility feature would be a migration instead
of a declaration.

## Proposed Change

### The declaration

```gof
module:
    Point
    distance
    Scale

import math

struct Point:
    x: int
    y: int

fn distance(a: Point, b: Point) -> int:
    return abs(a.x - b.x) + abs(a.y - b.y)

enum Scale:
    Small
    Large
```

Rules:

- `module:` at top level, followed by an indented block of export
  entries; it must be the **first** top-level declaration in the file,
  before any `import`, `struct`, `enum`, `fn`, `test fn`, `fixture(scope)
  fn`, `protocol`, or `impl`
- at most one `module:` block per file
- an entry is a **bare identifier**, one per line, newline-terminated,
  with the same block shape as struct fields and enum variants; no
  commas, no expressions, no qualifiers, no trailing colons
- entries may be resolved in any order relative to the declarations they
  name — the block is first, so every entry is a forward reference, the
  same order-independence RFC 0044 grants `impl` lines
- the block must contain at least one entry; gof has no empty blocks
  and no `pass`, and a module with nothing to export is not imported by
  anyone
- each entry names a **top-level declaration of this same file**:
  `struct`, `enum`, `fn`, or `protocol` names. Entries naming anything
  else — an unknown name, a name imported from another module, a
  `test fn` or `fixture(scope) fn`, a field or method name — are errors

Why this exact shape:

- `module` is **already reserved**. The lexer maps it
  (`"module" => TokenKind::Module`); the spec v1 surface lists it; and
  `parse_module` today has branches only for `import`, `struct`, `enum`,
  and `fn`, so a file starting with `module:` currently fails to parse.
  No compiling program can contain a top-level `module` token, and no
  identifier can collide with it, because it is a keyword. Activating a
  dead reserved token is the same zero-cost reservation story RFC 0044
  used for `protocol`; the only live use of the token today is the
  `fixture(module)` scope position, a distinct parser context that does
  not conflict
- `module:` nameless, not `module geometry:`. A gof module **is** a
  file; its import name is the file name or manifest alias, produced by
  the resolver, not by the source. A named header would invite
  file-name/module-name divergence (Go's mismatch problem) and a
  nestable-declaration surface gof does not have. The nameless form
  reads as "this file's public surface:", and it deliberately has zero
  effect on import resolution: `import name` still resolves
  same-directory, then package-root, then dependency, exactly per the
  spec's deterministic order. The block is a name-availability filter
  downstream of resolution, nothing more
- `:` + indented block, first in the file. Every gof top-level
  block declaration uses `Keyword ...:` + indent (`struct`, `enum`,
  `protocol`, `fn`), and every multi-item gof block lists one item per
  line. The export list follows both conventions, and each entry gets
  its own span for precise diagnostics. First position is the
  `package`-clause position: the public contract is read before the
  implementation, and "does this file opt in?" is answered by looking at
  line one, not by scanning. The cost — a one-line insert when a module
  grows an export list — is not a burden
- bare identifiers, no `exports` keyword, no comma list. An `exports`
  keyword or `module exports Point, distance` header adds a new token or
  a second list grammar for zero semantic gain. One bare name per line
  keeps the list diffable and greppable, and leaves room for future
  per-entry forms (an explicit rename/re-export entry is the natural
  growth point, deliberately not taken in v1) without grammar churn

### What an import contributes

One rule, with no exceptions, in both the grandfather window and the
final state:

> `import m` contributes exactly the top-level names `m` itself
> declares and exports. Nothing else — not `m`'s private declarations,
> and not the names `m` imported from other modules.

Consequences:

- **no transitive leakage.** `main` importing `lib` never sees `ops`
  names through `lib`, even when `lib` is blockless during the
  grandfather window. Today's closure-flat namespace (the `multiply`
  scenario in Problem) becomes a GOF3169 error at the use site with a
  trivial fix: add the missing direct `import ops`. This is the one
  immediate tightening the window cannot grandfather, because
  grandfathering it would mean two different leakage regimes before and
  after the flip; the rule stays singular instead
- **no re-export.** An export entry naming an imported name is rejected
  (GOF3168). A package that wants to forward a dependency's helper
  writes an explicit wrapper function. Re-export/facade lists (`pub use`,
  `from m import *` re-listed) are cut from v1 because they add
  indirection to the one question the flat namespace already answers
  poorly — "where does this name come from" — and because a re-export
  creates a second visible binding for one name, which interacts with
  the duplicate-name rules in a way that deserves its own RFC. Revisit
  trigger: a dedicated facade RFC once real package-facade needs exist
- the importer's namespace stays **flat** — imported names are used
  unqualified, exactly as today. This RFC adds no qualification syntax,
  no `m.name` paths, no aliased imports. Flatness is unchanged; what
  changes is the **set**: each module's visible names are its own
  declarations (public and private) plus the exports of its **direct**
  imports

### Per-module namespaces and the duplicate-name rules

Name availability moves from one graph-wide merged table to one table
per module: own declarations plus direct-import exports. The duplicate
rules re-scope accordingly:

- **same-file duplicates** stay errors under their existing codes
  (GOF3016 functions, GOF3020 structs, GOF3029 enums, GOF3021
  cross-kind, GOF3034 methods), with messages reworded from "in module
  graph" to "in module `name`"
- **local declaration vs imported export** in one module stays an error
  under the same existing family; the primary span moves to the local
  declaration (the one renamable without editing another module), the
  note points at the foreign export entry. Silent shadowing is not an
  option — a local `helper` and an imported `helper` colliding is
  exactly the ambiguity explicitness forbids
- **two direct imports exporting the same name** is the genuinely new
  composition error (GOF3171): neither declaration is local, so the
  error belongs to the importer's choices, reported at the import that
  completes the collision with notes at both export lists
- **same-named private declarations in different modules** are now
  legal. This is the point of privacy: private names never leave their
  file, so they cannot collide. It is a pure relaxation of today's
  graph-wide rule and breaks nothing that compiles
- import cycles (GOF3015), reserved stdlib name conflicts (GOF3108),
  manifest errors (GOF3089), and resolution failures (GOF3014) are
  untouched: visibility is applied after resolution succeeds

Method identity tightens to match: a method declaration
`fn TypeName.method(...)` requires the receiver struct to be declared
**in the same file** (GOF3170). Today the merged namespace lets any
module add methods to any visible struct, which is already in tension
with RFC 0044's rule that `impl` conformance must live in the struct's
own module — "the struct's own module is the single conformance
authority". Method sets must be that too, or a struct's method table
would depend on who imports it: if `a.gof` could add `Point.helper`,
then a module importing both `geometry` and `a` would see a different
`Point` than a module importing only `geometry`. Non-compositional
method sets are not compilable semantics; the declaration-side rule
must match the authority rule.

### Visibility granularity

Top-level declarations only. Exporting a struct exports its fields, its
methods, and (per below) its conformances as one unit; there is no
partial export and no `private` field or method in v1.

Defense:

- visibility governs the **flat top-level namespace** — the unqualified
  names a module contributes to importers. Fields and methods are not
  in that namespace; they are reached through a receiver
  (`point.x`, `point.total()`), and the receiver's type is the scope
  that governs them. Two different name-resolution systems should not
  share one visibility mechanism
- a struct is one contract. "Here is `Point`, but you may not read `x`"
  is a data-encapsulation feature, not a module-boundary feature; gof
  already has the tools that make it meaningful only later (ownership,
  protocols). Folding it in now would create a combinatorial API surface
  (field-level, method-level, per-mutability) with no use case that
  survives the v1 feature set
- the flat namespace's actual cost — name collision management across
  modules — is solved exactly by top-level filtering. Private helper
  functions, internal struct names, and internal enums are the real
  leak; private fields would not reduce any import collision

Consequence: a struct's method set is visible wherever the struct is
visible. Importing `Point` gives you every method declared on `Point`
in its own module, including ones the module's author uses internally.
That is honest: a method callable through a value of the type is part of
the type's contract; hiding it would require receiver-typed values to
carry provenance, which is runtime state for a compile-time question.

### Protocol conformances

Decision: a conformance `Struct: Protocol` is usable from any module in
which **both** `Struct` and `Protocol` are visible (own declaration, or
direct import of a module exporting them). A conformance is not a name:
it cannot be an export entry, is not re-exported, and is never visible
through an import that provides only one endpoint.

Defense:

- RFC 0044 already fixed the authority: `impl` must be declared in the
  conforming struct's own module, and foreign `impl`s are errors
  (GOF3133). So every conformance is declared exactly once, at the
  struct's home, and no set of imports can produce disagreement. Since
  the struct side is only reachable by importing the struct's module,
  "both visible" reduces to: import the struct's module, and see the
  protocol
- every use site already requires both names syntactically. Writing a
  generic bound `[T: Shape]` needs `Shape` in scope; passing `Circle`
  needs `Circle` in scope. The rule therefore adds no new failure mode
  and needs no new diagnostic: a module that cannot name one endpoint
  cannot express the conformance use in the first place, and existing
  codes (unknown protocol in a bound, GOF3132; type does not satisfy a
  bound, GOF3134) cover the rest. This is also why the rule is
  future-proof: if existentials arrive post-v1, "both visible" is the
  correct admission rule for accepting `Circle` where `Shape` is
  expected
- the weaker alternative — conformance usable only where declared —
  would break RFC 0044's own cross-module plan (protocol in one module,
  conformance and generic use elsewhere) and make imported types useless
  as generic arguments. The stronger alternative — visible wherever
  either endpoint is visible — leaks a fact to modules that cannot even
  write it, which is meaningless access

Protocol **names** are top-level names like struct names: they are
exportable entries, they join the duplicate rules exactly as RFC 0044
extended them to the top-level namespace, and a module may keep a
protocol private (unexported) while exporting conforming structs —
importers then see the struct and its methods, but cannot write bounds
against the protocol they cannot name. No diagnostic fires for that;
name availability is the whole rule.

### Entrypoints, tests, and the test surface

- `src/main.gof`'s `fn main` is invoked by the toolchain, not imported;
  visibility never gates entrypoints. A blockless `main.gof` after the
  flip is simply a fully private file whose own `main` still runs
- `src/lib.gof` is the dependency entrypoint: its `module:` block is the
  dependency's public API. After the flip a dependency without a block
  exports nothing; during the window it exports all its own declarations
  (today's behavior)
- `test fn` and `fixture(scope) fn` cannot be export entries (GOF3168).
  They are the test surface, discovered per-file by `gof test` from
  `*_test.gof` and `tests/**/*.gof`, not API. They still participate in
  their own file's namespace for collision purposes, exactly as today
- test files are module-graph roots like any entrypoint: each `*_test.gof`
  file compiles against its own imports, and post-flip its imports must
  export the names the tests use. Repo test fixtures migrate with the
  stdlib (Test Plan)
- doctests and `examples/` are their own module graphs and follow the
  same rule; docs sync is an implementation-slice obligation

### Builtin shadowing

The spec's rule — a top-level user function with a builtin's name
shadows the builtin call — becomes per-module-**visible**: the shadow
applies only in modules where that function is visible (own declaration
or imported export). Today the merged namespace makes a helper named
`print` in any module shadow `print` graph-wide; after this RFC a
private `print` helper shadows only inside its own file. This is a
tightening of a hidden global behavior and a direct AGENTS.md §5 win.
Builtin helper names themselves are unchanged: they are not module
scoped, so stdlib method bodies calling `__gof_internal_*` hooks are
unaffected.

### Implementation layers

No layering hacks; the feature splits along the existing pipeline:

- **lexer**: no change — `TokenKind::Module` exists
- **CST/AST**: `Module` gains an export list (`name` + `span` per
  entry); `parse_module` dispatches on `module` before other
  declarations and reuses the existing indented-block machinery (the
  struct-field parser's shape). The parser enforces syntax only:
  first-position, at-most-one, bare-identifier entries, non-empty block
- **module graph**: after parsing a file, the load step checks the entry
  contract (each entry names an exportable declaration of that file) and
  builds the per-module visible-namespace tables that replace the single
  `collect_items` merge. This layer already owns duplicate detection,
  cycle detection, and resolution order, so ownership is unchanged —
  only the shape of its output changes, from one merged item list to
  per-module tables plus the reachability set
- **typed HIR / typechecking**: name resolution consumes the per-module
  tables; use-site privacy errors (GOF3169), import collisions
  (GOF3171), and receiver locality (GOF3170) surface here and in the
  module-graph checks, with spans, notes, and fix-its. The bootstrap
  evaluator resolves calls through the same tables, so evaluator and
  compiler agree by construction
- **MIR/backend**: unchanged. Visibility has no runtime representation.
  In the Phase 2 native pipeline, non-exported module symbols take
  internal linkage and become dead-code-eliminable when unreachable

### AGENTS.md §5 filter

- **who owns the data** — each module owns its public surface as a
  compile-time fact, declared once at the top of its file. Export lists
  and visibility tables are compiler-owned metadata; no value, object,
  or runtime structure changes ownership
- **where values live** — unchanged. Visibility is name availability,
  decided and discarded before codegen; values keep their existing
  taxa, layouts, and residences
- **where allocation happens** — nowhere new. All visibility work is
  compile-time; export tables are build-side maps. The evaluator and
  runtime allocate nothing they do not allocate today
- **where blocking can happen** — nowhere new. Every check runs during
  compilation; no runtime participates in visibility at all
- **how races are prevented** — no shared state is introduced. Export
  lists are immutable per-module inputs; per-module namespace tables
  are pure functions of the module graph. Nothing mutable crosses a
  module boundary
- **how it is debugged** — every failure is a compile-time diagnostic
  with a span, a note pointing at the declaration or export entry
  involved (including its file and line), and a fix-it. The
  grandfather-window warning (GOF3172) makes migration state visible
  per import, and `gof mod export` prints the block a file needs

## Alternatives Considered

### Default-public with `private`/`internal` markers

The (a) direction: everything stays importable unless marked `private`.

Rejected. It inverts the explicitness argument: the **public API is the
contract other modules rely on**, so the public surface — not its
absence — is what should be spelled out. Negative markers scale as O(N)
scattered annotations versus one table; privacy stops being greppable
("which helpers are internal?" becomes a file-by-file audit); any file
that forgets a marker silently publishes internals — and under gof's
flat namespace, publishes them graph-wide. The repo's own spec already
maintains hand-written per-module exposure lists, which is direct
evidence that the export list is the shape the contract naturally takes.
Marker keywords are also two new reserved words for a strictly weaker
discipline.

### Per-declaration `public` / `pub` prefixes

Rust-style `pub fn`, `pub struct`. Rejected: same default-public
inversion as above, plus it scatters the API across the whole file. A
reader cannot answer "what does this module export" without reading
every declaration. The `module:` block answers it in one place and
doubles as the file's table of contents.

### Go-style exported-by-capitalization

Rejected outright. Encoding visibility in identifier case is implicit
semantics hidden in a naming convention — precisely the kind of invisible
rule gof exists to refuse — and it would retroactively change the
meaning of every existing capitalized identifier (`Point`,
`TestContext`, every stdlib name).

### `module exports Point, distance` header or an `exports` keyword

Rejected. A comma list on the header line breaks gof's universal
"top-level `:` introduces an indented block" convention, and a new
`exports` keyword spends token surface for zero semantics. The
nameless `module:` block reuses one dead reserved token and the
existing block grammar.

### Named `module geometry:` headers

Rejected. Modules are files; the import name comes from the resolver
(file name, package root, manifest alias). A source-level name invites
name/file divergence and a nesting surface gof does not have. See the
defense in Proposed Change.

### Edition-gated compatibility flip

`gof.mod` already carries `edition = "2026"`; gating default-private on
a new edition value is technically available.

Rejected for this change. Editions exist to let **post-1.0** code escape
breaking changes; spending the mechanism pre-freeze would fragment a
language that has not yet stabilized anything, creating two coexisting
visibility regimes with no second edition to migrate from and no
conformance story for either. The honest pre-v1 answer is a scheduled
default flip at the freeze, announced in advance, carried by tooling.
Forward guidance: after v1, any change to visibility defaults goes
through editions, never silently.

### Permanent grandfather (default-public forever for blockless files)

Rejected. It makes the two-regime state permanent: every unconverted
file keeps leaking its whole surface into the flat closure, the
duplicate-name rules stay graph-wide for those files, and the builtin
shadowing leak survives. gof would carry the exact ambiguity this RFC
removes as a permanent dialect split.

### Field/method-level visibility

Rejected for v1; see the granularity defense in Proposed Change. The
revisit trigger is real data-encapsulation needs (invariant enforcement
on struct fields), which belong to the ownership/type-system line, not
the module boundary.

### Re-export and facade entries

Cut from v1; see "What an import contributes". The explicit wrapper
function is the honest v1 facade. Revisit trigger: a dedicated RFC that
defines re-export binding semantics against the duplicate-name rules.

## Compatibility Impact

Honest, staged, with the one immediate tightening called out:

- **no syntax regression.** `module` is a reserved keyword today with no
  parser rule, so no compiling program can contain a top-level `module:`
  — activation cannot regress the corpus (verified against the parser:
  `parse_module` has no `module` branch, so such files fail to parse
  today)
- **grandfather window (RFC lands → v1 freeze).** A file without a
  `module:` block has all its **own** top-level declarations importable,
  preserving today's per-module default. All repo programs, the six
  stdlib `.gof` modules, examples, fixtures, and doctests keep compiling
  unchanged. During the window, `module:` files opt into default-private
  semantics
- **the one immediate tightening.** Transitive leakage dies now, for
  blockless files too: a name from a module the importer did not
  directly import becomes GOF3169, with the fix "add the direct import".
  The alternative — grandfathering the flat closure for blockless files —
  would mean leakage semantics change twice. Programs that rely on the
  closure (using a dependency's dependency unqualified) break; the repo
  conformance suite is the verification, and the fix is mechanical and
  an improvement
- **method locality.** Declaring a method on a struct from another
  module becomes GOF3170. Legal today, but already contradicted by RFC
  0044's same-module authority rule for `impl`. The migration is to move
  the method to the struct's file or make it a free function
- **relaxations.** Same-named private declarations in different modules
  become legal (previously graph-wide duplicates), and builtin shadowing
  becomes per-module-visible instead of graph-wide. Neither breaks any
  compiling program
- **the v1 freeze flip.** At Phase 1 exit ("core frozen"), the default
  inverts: a file without a `module:` block exports nothing. This is a
  deliberate, scheduled breaking change at the one contractually
  reserved moment — pre-freeze, the spec explicitly keeps the surface
  mutable, and VISION.md names this RFC as Phase 1 work, so the freeze
  is the last honest point to lock the default. It is defensible only
  with a tool and a warning, so both ship in this RFC: `gof mod export`
  emits the block a file needs (all top-level names; a human prunes),
  and GOF3172 warns on every import of a blockless module for the whole
  window. Stated plainly: packages that ignore the warning break at the
  freeze. That is the cost of not spending an edition on it, and it is
  paid once, at a moment gof has already declared breakable
- **stdlib dogfooding.** The six stdlib modules gain `module:` blocks
  matching the exposure lists `spec/language-v1.md` already documents —
  the prose contract becomes the enforced contract, and `gof test` on
  the repo proves the feature on real surface area before the freeze
- **unchanged surfaces.** Import resolution order, import cycles,
  reserved stdlib names, `gof.mod`/`gof.lock` semantics, `go` spawn
  targets, fixture lifetimes, and the evaluator's observable behavior
  are all untouched. `src/main.gof` entrypoints never need a block
- **spec sync obligations** for the implementation slice: `spec/language-v1.md`
  moves `module` from the dead v1-surface listing into the implemented
  subset with the binding rules above and the import-contribution rule;
  `spec/diagnostics.md` gains the five new codes; `roadmap.md` records
  the Phase 1 visibility item and the freeze-flip date; `docs/site/`
  and `examples/` gain a module-visibility walkthrough; the VS Code
  extension grammar, snippets, and tests are updated per AGENTS.md §9
  with `npm test` and `npm run package` verified

### Implementation phasing amendment (review-adjudicated)

The v1 implementation ships the **declaration-side contract** in full
and defers the **namespace-enforcement side**:

Shipped (commits on `agent/language-completion`):

- `module:` block syntax: first-position validation, at-most-one,
  non-empty, bare-identifier entries, duplicates rejected — all GOF3168
- entry-contract validation: entries naming unknown, imported, `test
  fn`, or `fixture fn` names report GOF3168
- method locality (GOF3170): `fn Type.method` requires the receiver
  struct in the same file, with the builtin-opaque exception for
  shipped stdlib method surfaces
- import collisions (GOF3171): two direct imports contributing the same
  plain-function/struct/enum/protocol name; methods are keyed by
  `(receiver, name)` and never collide by bare name
- grandfather window (GOF3172): every import of a blockless module
  warns through the warnings channel (warning severity never fails
  compilation), with the `module:` fix-it

Deferred with re-entry criteria: the strict per-module namespace —
export-block filtering of contributions, no-transitive-leakage, and
GOF3169 — lands when the typed layer gains per-module body views (each
function's body resolving against its own module's visible set). The
current flat merge exists because imported modules' bodies typecheck
against the one merged table; filtering a name out of that table breaks
the importing module's own body before it protects any importer.
Re-entry trigger: per-module compilation in the native backend (ADR
0004's per-module parallel codegen), which requires per-module views
anyway. Until then, a `module:` block validates and documents the
public surface but does not yet hide the private names from the flat
merge; the v1-freeze flip therefore also defers to the same trigger.

## Diagnostics Impact

Five codes, allocated after the last reserved GOF3157 (GOF3158–GOF3167
stay reserved for the RFC set in flight); this RFC allocates
GOF3168–GOF3172 and nothing else. All follow the diagnostics contract
(code, severity, primary span, message, note, optional fix-it) and are
deterministic:

- `GOF3168` (error): invalid `module:` export declaration. Covers: a
  block that is not the file's first top-level declaration; a second
  block; an empty block; entries that are not bare identifiers; entries
  naming an unknown name, a name imported from another module
  (re-export attempt), a `test fn` or `fixture(scope) fn`, or a
  field/method; duplicate entries. The parser reports the placement and
  shape triggers; the module-graph load step reports the entry-contract
  triggers with a note naming the nearest matching declaration when one
  exists
- `GOF3169` (error): use of a name an imported module does not export —
  "name `helper` is not exported by module `geometry`". Primary span at
  the use site; note points at the declaration in the exporting module
  (file:line) and at its `module:` block (or its absence after the
  flip); fix-it: "add `helper` to `geometry`'s `module:` export list, or
  import the module that exports it, or remove the use". This is also
  the code for post-flip use of any name from a blockless module and
  for pre-flip use of closure-leaked names
- `GOF3170` (error): method declared on a struct from another module —
  "receiver type `Point` is declared in module `geometry`". Notes point
  at the struct declaration and mirror RFC 0044's same-module authority
  rule for `impl`. Fix-it: "move the method into `geometry.gof` next to
  `Point`, or declare a free function instead"
- `GOF3171` (error): import-vs-import export collision — "imports `x`
  and `y` both export `helper` into module `main`". Primary span at the
  import that completes the collision; notes at both export entries.
  Fix-it: "rename one exported name or remove one import"
- `GOF3172` (warning): grandfather-window migration signal — "import
  `geometry` resolves to a module with no `module:` export block; all
  its own declarations are currently importable, and the default flips
  to private at the v1 freeze". Fix-it: "add a `module:` block to
  `geometry.gof` (run `gof mod export`)". Window-only: the code retires
  at the flip, where GOF3169 takes over. This is the repo's first
  warning-severity diagnostic; `Severity::Warning` already exists in
  `diagnostics.rs`, and the slice adds a `Diagnostic::warning`
  constructor — one function, no new mechanism

Existing codes change scope, not identity: GOF3016, GOF3020, GOF3021,
GOF3029, and GOF3034 keep their codes with messages reworded from "in
module graph" to "visible in module `name`", and the local-vs-import
collision case reports the local declaration as primary. Everything
else the feature can produce reuses existing codes: unknown protocols
in bounds stay GOF3132, unsatisfied bounds stay GOF3134, foreign `impl`
targets stay GOF3133, resolution and cycle failures stay GOF3014 and
GOF3015. `spec/diagnostics.md` gains the five codes when the
module-graph slice lands.

## Test Plan

- parser / CST / AST tests: `module:` block parsing at first position;
  bare-identifier entries; forward references; reuse of the indented
  block machinery; rejection fixtures (late block, second block, empty
  block, comma lists, qualified entries) each pinning GOF3168
- module-graph unit tests, extending the existing suite in
  `module_graph.rs`: per-module namespace construction; the
  `loads_functions_from_local_package_dependencies` scenario re-pinned
  with `multiply` now requiring a direct import (GOF3169); export
  filtering of imports; private same-name declarations in two modules
  now legal; import-vs-import collision (GOF3171); local-vs-import
  collision reusing GOF3016/GOF3021 with updated messages
- entry-contract checks: entries naming unknown, imported, test, or
  fixture names; duplicate entries — one negative fixture per GOF3168
  trigger with expected spans, notes, and fix-its pinned
- method locality: `fn Point.total` declared in a module that imports
  `Point` → GOF3170; same-file methods unchanged; duplicate methods in
  one file still GOF3034
- protocol interplay per RFC 0044: exported protocol + exported struct,
  `impl` in the struct's module, generic call from a third module that
  imports both — compiles; struct exported but protocol unimported —
  the bound is an unknown protocol under existing codes, no new
  diagnostic; private protocol with conforming exported struct —
  conformances unusable but methods callable, documented as a fixture
- test surface: `test fn` / `fixture(scope) fn` not exportable
  (GOF3168); `gof test` discovery and fixture caching unchanged across
  the change; `*_test.gof` roots resolve imports through export tables
- stdlib migration: the six `stdlib/*.gof` files gain `module:` blocks
  matching the spec's documented exposure lists exactly; the
  `stdlib_imports` pass fixture and the stdlib doctests stay green
- evaluator behavioral conformance: identical results before and after
  the visibility tables land, proving visibility is compile-time only
- CLI: `gof mod export` emits correct blocks for sample files; GOF3172
  fires on imports of blockless modules and disappears after adding
  blocks; `gof build --native` source bundles unchanged
- VS Code extension (AGENTS.md §9): grammar and snippets for `module:`
  blocks; `npm test` and `npm run package` verified
- docs sync: `gof test --docs` runs the new module-visibility
  walkthrough in `docs/site/`/`examples/`

## Benchmark Impact

- **dev compile budget.** Per-module namespace tables replace the single
  `collect_items` merge at the same O(graph) cost — the graph is walked
  once either way. Gate: pinned incremental rebuild budgets (VISION.md:
  small edits keep small compile budgets) do not move; edits that do not
  touch a `module:` block or an import list must not rebuild importers'
  symbol tables. An export-list edit invalidates importers, exactly as
  any public-API change already must — that is inherent to the contract,
  not a regression
- **runtime.** Zero by construction: no codegen, evaluator, or runtime
  change. Assertion-backed: evaluator conformance runs produce identical
  results, and no new allocation exists at runtime (AGENTS.md §5)
- **binary size (Phase 2 native gate).** Non-exported module symbols
  take internal linkage and become dead-code-eliminable when
  unreachable. Gate: the same scorecard workload compiled
  export-disciplined versus all-public produces equal-or-smaller
  release binaries, with the delta reported honestly even if it rounds
  to zero for the evaluator-era artifacts
- **no throughput or concurrency surface.** Visibility adds no runtime
  construct, so no dispatch, allocation, or scheduling benchmark is
  affected; the only measurable claims are the compile-budget and
  binary-size gates above, and both are pinned rather than asserted
