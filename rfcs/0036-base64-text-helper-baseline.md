# RFC 0036: Base64 text helper baseline

## Summary

Add `base64_encode(text)` and `base64_decode(text)` as explicit bootstrap
helpers for automation, HTTP, and CI-style scripting workflows.

## Problem

The current script-first stdlib already covers config parsing, JSON, CSV,
templating, process execution, stdin, and wall-clock time. A common remaining
gap for real automation work is moving text payloads through explicit base64
steps for headers, tokens, and serialized text blobs.

Without a builtin baseline, users are forced into:

- ad hoc manual encoding tables in user code
- shelling out to external tools for a tiny transformation
- pretending binary/text conversion rules do not matter

That is bad product shape for a language trying to compete on correctness-first
automation workflows.

## Proposed change

Add two helpers:

- `base64_encode(text) -> string`
- `base64_decode(text) -> Result[string, RuntimeError]`

`base64_encode(...)` is pure and infallible for UTF-8 text, so it returns a
plain `string`.

`base64_decode(...)` is fallible and therefore returns `Result`. It must reject:

- invalid base64 input
- decoded bytes that are not valid UTF-8

These failures surface as `RuntimeError.Base64(message)`.

## Public surface

New builtin functions:

```gof
encoded = base64_encode("gof!")
decoded = base64_decode(encoded)?
```

New builtin runtime error variant:

```gof
RuntimeError.Base64(message: string)
```

## Semantics

- `base64_encode(text)` accepts exactly one `string` and returns a base64
  encoded `string`
- `base64_decode(text)` accepts exactly one `string` and returns
  `Result[string, RuntimeError]`
- decoding uses the standard base64 alphabet
- `base64_decode(...)` is text-only in the bootstrap stage; it does not invent a
  bytes type or raw binary container
- invalid base64 text and decoded non-UTF-8 bytes are explicit operational
  failures, not diagnostics

## Alternatives considered

### Add a bytes type first

Rejected for this slice. A bytes model belongs to a broader native/runtime
design step and should not be smuggled in through one helper pair.

### Shell out to platform tools

Rejected. That would be less deterministic, less portable, and worse for the
language's script-first UX.

## Compatibility impact

The change is additive.

The only visible enum-surface effect is the new `RuntimeError.Base64(...)`
variant, which requires exhaustive matches over `RuntimeError` to add one arm.

## Diagnostics impact

- wrong arity still uses `GOF3005`
- invalid operand types use `GOF3105`
- runtime failures use `RuntimeError.Base64(message)`

## Out of scope

- bytes/buffer types
- URL-safe base64 alphabets
- streaming encoders/decoders
- file-oriented binary helper APIs

## Test plan

- type-layer validation for both helpers
- interpreter roundtrip test
- interpreter runtime failure test for invalid decode input
- pipeline test ensuring both calls lower and type correctly
- CLI example smoke test
- docs/example/VS Code surface sync
