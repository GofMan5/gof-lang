# ADR 0006 — Backend IR v1: linear SSA form and binary encoding

## Status

Accepted (Phase 2, slice 1).

## Context

ADR 0004 fixed the backend strategy: dual-profile codegen from a
serializable linear SSA IR (CLIF/LLVM-IR class), where serialized IR is
the incremental-cache boundary in a compact binary zero-copy format.
The current pipeline ends at `ssa.rs`: a marker-delimited linear form
with STRING-named values and structured-control-flow markers
(`BeginIf`/`Else`/`EndIf`/...), serialized as JSON inside the
transitional `bootstrap-ir-v0` artifact. That form cannot serve
codegen: string keys, no block discipline, no terminator algebra, and
JSON is not a cache format.

## Decision

A new module `backend_ir` defines IR v1, lowered one-way from
`SsaModule`:

1. **Block-based linear SSA.** A function is an ordered list of basic
   blocks; every block ends in exactly one terminator
   (`Jump`, `Branch`, `Switch`, `Return`, `Unreachable`). Control-flow
   markers in the current SSA lower into explicit block boundaries; a
   block discovery pass splits at markers and produces the branch
   structure. Dominance is implied by order + terminators, never by
   nesting.

2. **Numeric value identities.** Values are `u32` ids assigned in
   first-definition order (`v0, v1, ...` per module). Instructions
   reference ids only — no strings in operands. Symbols (function
   names, field names, enum/variant names) live in a per-module string
   table; instructions reference symbols by `u32` index.

3. **Instruction algebra (v1 scope).** Constants (int/string/bool/enum
   variant), aggregates (list/dict/struct), loads (local/field),
   stores, unary/binary arithmetic with the AST operator set, calls,
   index, spawn, await, propagate, match dispatch (block-pair switch
   with binding mapping), and `Eval` for statement-position
   expressions that produce no value. The algebra is closed under what
   the MIR/SSA layer emits today; growth is additive.

4. **Binary encoding (the cache boundary).** A hand-rolled
   deterministic byte format — no JSON, no text dumps:
   - header: magic `GOFIR`, format version `u16` (v1 = 1)
   - varint (LEB128) integers throughout
   - string table: count + (len + UTF-8 bytes) per entry
   - module: function count; per function: name symbol, param ids,
     return type tag, block count; per block: id, terminator tag +
     operands, instruction count; per instruction: tag + result-slot +
     operands (ids, varints, symbol indexes)
   - the encoding is canonical: identical IR encodes to identical
     bytes (byte-stable round-trip), so cache invalidation can be
     content-hash based
   - decoding validates tags, operand kinds, and id bounds; malformed
     input is an error, never UB

5. **Zero-copy class, not yet mmap.** v1 decodes into owned structures
   with a single validation pass (postcard-class). The format's stable
   layout and id-only operands are the mmap/zero-cast prerequisite;
   upgrading the reader to cast-without-copy is a later
   performance slice that does not change the format.

6. **Pipeline position.** `source → AST → HIR → typed HIR → MIR → SSA
   → backend IR` — the lowering `backend_ir::lower(&SsaModule)` is
   one-way (ADR 0002 discipline); the old `bootstrap-ir-v0` JSON
   artifact stays as the CLI's current `build` output until the
   Cranelift artifact passes its smoke gates (ADR 0004 retirement
   trigger), but the IR and its binary cache land NOW so the backend
   work has its contract.

## Testing contract

- round-trip: for every fixture module, `lower → encode → decode`
  reproduces the IR exactly (structural equality), and
  `encode(decode(encode(ir))) == encode(ir)` (byte-stable)
- determinism: two lowers of the same source produce identical bytes
- validation: fuzz-ish malformed inputs (truncated, bad tags, out-of
  -range ids/symbols) decode to errors, never panics
- block discipline: every block ends in exactly one terminator; every
  branch target exists; every value id used is defined before use in
  block order

## Consequences

- the incremental cache has its format and its invalidation rule
  (content hash) from day one
- the codegen slice compiles from a real block/terminator algebra,
  not from markers
- `bootstrap-ir-v0` JSON is now formally transitional with a named
  successor
- the format owns a version field; additive growth (new instruction
  tags) bumps minor versions, layout changes bump the major and
  require cache invalidation by design
