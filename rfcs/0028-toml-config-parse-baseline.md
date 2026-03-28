# RFC 0028: TOML config parse baseline

- Status: Accepted
- Area: stdlib, runtime, typechecker, docs
- Created: 2026-03-28

## Problem

After JSON, CSV, filesystem, and package-locking slices, `gof` still lacked a
first-class configuration format for real internal tooling and service-style
bootstrap apps.

TOML is the right next step because it improves:

- local tool configuration
- manifest-adjacent automation flows
- human-edited settings files

without forcing the language to commit to a full schema/serde subsystem yet.

## Proposed change

Add bootstrap `toml_parse(text)` as an explicit configuration-ingest helper:

- `toml_parse(text) -> Result[json, RuntimeError]`

The helper parses a TOML document and bridges the supported value surface into
the existing bootstrap `json` value model.

### Public surface

- new builtin: `toml_parse(text)`
- accepted argument type: `string`
- return type: `Result[json, RuntimeError]`
- operational failures surface as `RuntimeError.Toml(message)`

### Semantics

- the input is parsed as a full TOML document, not as a single TOML scalar
- successful parses bridge into bootstrap `json`
- supported TOML value classes in this slice:
  - strings
  - integers
  - booleans
  - arrays of supported values
  - tables with supported values
- unsupported TOML value classes in this slice:
  - floats
  - datetimes

Unsupported scalar classes are rejected explicitly with
`RuntimeError.Toml(message)` instead of being silently coerced.

This keeps the cost model and value semantics honest while the bootstrap `json`
type remains intentionally narrow.

## Alternatives considered

- introducing `toml_stringify(...)` in the same slice
  - rejected because parse-only config ingest is the immediate need, while
    stringify requires a larger value-contract decision
- decoding TOML directly into structs
  - rejected because bootstrap `gof` does not yet have a stable schema/serde
    layer
- silently coercing TOML floats or datetimes into strings
  - rejected because it would hide data-shape changes and weaken
    predictability

## Compatibility impact

- additive change only
- existing code paths are unchanged
- `RuntimeError` gains one new operational variant:
  `RuntimeError.Toml(message: string)`
- exhaustive `RuntimeError` matches in examples and fixtures must account for
  the new variant

## Diagnostics impact

- `GOF3005`: wrong number of arguments for `toml_parse`
- `GOF3099`: invalid operand for bootstrap TOML helpers

## Out of scope

- `toml_stringify(...)`
- schema validation
- serde-style automatic struct decoding
- float support in the bootstrap `json` bridge
- datetime support in the bootstrap `json` bridge

## Test plan

- typechecker coverage for accepted typing and invalid operand diagnostics
- interpreter coverage for:
  - successful document parsing
  - unsupported scalar rejection
- pipeline coverage for compile-through of `toml_parse(...)`
- runnable example plus CLI regression coverage

## Benchmark impact

No new benchmark gate is introduced in this slice.

The current helper is bootstrap stdlib surface for config ingestion, not a hot
runtime path. If TOML parsing starts to appear in startup-critical service
paths, benchmark coverage should be added together with broader config/runtime
work in M9.
