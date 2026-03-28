# RFC 0035: `yaml_parse()` baseline

- Status: Accepted
- Area: stdlib, runtime, typechecker, docs
- Created: 2026-03-28

## Problem

After JSON, CSV, TOML, templating, stdin, and explicit time/process helpers,
`gof` still lacked first-class support for one of the most common automation and
CI configuration formats.

YAML appears in:

- CI pipelines
- container and orchestration configs
- deployment manifests
- internal automation metadata

Without a narrow builtin bridge, `gof` scripts still have to shell out or rely
on preconverted JSON for a large class of real-world automation tasks.

## Proposed change

Add bootstrap `yaml_parse(text)` as an explicit config/data-ingest helper:

- `yaml_parse(text) -> Result[json, RuntimeError]`

The helper parses YAML and bridges the supported value surface into the existing
bootstrap `json` model.

### Public surface

- new builtin: `yaml_parse(text)`
- accepted argument type: `string`
- return type: `Result[json, RuntimeError]`
- operational failures surface as `RuntimeError.Yaml(message)`

### Semantics

- the input is parsed as a YAML document
- successful parses bridge into bootstrap `json`
- supported YAML value classes in this slice:
  - `null`
  - booleans
  - integer numbers
  - strings
  - sequences of supported values
  - mappings with string keys and supported values
- unsupported YAML value classes in this slice:
  - non-integer numeric values
  - mappings with non-string keys
  - tagged values

Unsupported YAML shapes are rejected explicitly with
`RuntimeError.Yaml(message)` instead of being silently coerced.

## Alternatives considered

- adding `yaml_stringify(...)` in the same slice
  - rejected because parse-only automation/config ingest is the immediate need
- silently coercing YAML mapping keys into strings
  - rejected because it hides data-shape changes and weakens predictability
- decoding YAML directly into structs
  - rejected because bootstrap `gof` still has no stable schema/serde layer

## Compatibility impact

- additive change only
- existing code paths are unchanged
- `RuntimeError` gains one new operational variant:
  `RuntimeError.Yaml(message: string)`
- exhaustive `RuntimeError` matches in examples and fixtures must account for
  the new variant

## Diagnostics impact

- `GOF3005`: wrong number of arguments for `yaml_parse`
- `GOF3104`: invalid operand for bootstrap YAML helpers

## Out of scope

- `yaml_stringify(...)`
- anchors and merge-key convenience layers
- struct decoding
- schema validation
- typed datetime or float bridging

## Test plan

- typechecker coverage for accepted typing and invalid operand diagnostics
- interpreter coverage for:
  - successful YAML parsing
  - unsupported YAML shape rejection
- pipeline coverage for compile-through of `yaml_parse(...)`
- runnable example plus CLI regression coverage

## Benchmark impact

No new benchmark gate is introduced in this slice.

The helper is bootstrap stdlib surface for automation/config ingestion, not a
hot runtime path. If YAML parsing becomes startup-critical in service paths,
benchmark coverage should be added together with broader config/runtime work in
M9 and M10.
