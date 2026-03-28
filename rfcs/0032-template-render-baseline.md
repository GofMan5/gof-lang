# RFC 0032: Template Render Baseline

- Status: Accepted
- Created: 2026-03-28
- Area: stdlib, scripting, data-wrangling

## Summary

Add `template_render(template, values)` as a small, explicit templating baseline
for script-heavy `gof` programs.

The helper is intentionally narrow:

- input template syntax is `{{key}}`
- lookup is top-level only
- values come from either `dict[...]` or a top-level `json` object
- failures stay explicit through `Result[string, RuntimeError]`

This is not a general-purpose template engine. It is a deterministic text
rendering helper for internal automation and config/report generation.

## Motivation

M9 explicitly calls out data wrangling and scripting coverage. JSON, CSV, TOML,
filesystem helpers, and process orchestration already exist, but there was still
no first-class path for turning structured data into output text without
hand-built string concatenation.

That gap hurts exactly the automation scenarios `gof` wants to serve:

- config-derived command generation
- status/report text assembly
- small internal notification payloads
- CLI scaffolding and lightweight file generation

## Design

Public contract:

```gof
template_render(template: string, values: dict[...]) -> Result[string, RuntimeError]
template_render(template: string, values: json) -> Result[string, RuntimeError]
```

### Placeholder syntax

- `{{key}}` resolves `key`
- surrounding whitespace inside the placeholder is ignored
- empty placeholders are rejected
- nested braces inside one placeholder are rejected
- unclosed `{{` is rejected

### Context rules

- `dict[...]` context:
  - lookup uses the existing deterministic string-key dict contract
  - printable values are rendered directly
  - non-printable runtime values such as channels/tasks/cancel tokens are rejected
- `json` context:
  - only a top-level JSON object is accepted
  - JSON string values render without JSON quotes
  - JSON ints/bools/null render as their textual form
  - JSON arrays/objects render as compact JSON text

### Failure mode

All runtime templating failures return:

- `Result.Err(RuntimeError.Template(message))`

This keeps text rendering explicit and composable with existing `Result` flows.

## Non-goals

This RFC deliberately does not add:

- nested lookup like `{{config.port}}`
- loops or conditionals
- escaping rules beyond the literal non-placeholder text
- custom filters/functions
- HTML-aware escaping
- framework-style global template state

Those would move the design from “explicit script helper” toward a template
engine surface, which is outside the current bootstrap scope.

## Why not import a full template engine

The language contract here is smaller than a general-purpose library surface.

`gof` needs:

- deterministic behavior
- simple cross-layer typing
- explicit failure reporting through `RuntimeError`
- no hidden global state or DSL semantics beyond the documented subset

Pulling in a full template engine at this stage would add more surface and more
future compatibility burden than the current scripting milestone justifies.

## Testing

The baseline is covered through:

- type/pipeline coverage for builtin recognition and return type
- interpreter success tests for dict and JSON contexts
- negative tests for invalid context types
- runtime-result tests for missing placeholders
- CLI example coverage through `examples/template_report.gof`

## Roadmap impact

This closes a concrete part of M9 data transformation work:

- `CP-M9-3a`: explicit `template_render(template, values)` baseline for
  dict/json-driven text generation in scripts
