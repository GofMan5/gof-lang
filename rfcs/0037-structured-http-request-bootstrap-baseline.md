# RFC 0037: Structured HTTP request bootstrap baseline

- Status: Accepted
- Area: stdlib, runtime, typechecker, docs
- Created: 2026-03-28

## Problem

`gof` already has bootstrap `http_get(...)` and `http_post(...)`, which are
enough for simple long-polling and webhook examples. They are not enough for
real automation and bot workflows that need:

- explicit request methods
- custom request headers
- timeout control
- access to response headers
- non-2xx handling without collapsing everything into a failure path

Without a structured request helper, scripts still have to fall back to host
glue for a large class of operational HTTP/TLS work.

## Proposed change

Add bootstrap `http_request(...)` as an explicit structured request helper:

- `http_request(method, url) -> Result[json, RuntimeError]`
- `http_request(method, url, body) -> Result[json, RuntimeError]`
- `http_request(method, url, body, headers) -> Result[json, RuntimeError]`
- `http_request(method, url, body, headers, timeout_ms) -> Result[json, RuntimeError]`

The helper keeps the existing bootstrap transport stack and returns a structured
JSON report instead of only a response body string.

## Public surface

- new builtin: `http_request(...)`
- accepted argument types:
  - `method: string`
  - `url: string`
  - optional `body: string`
  - optional `headers: dict[string]`
  - optional `timeout_ms: int`
- return type: `Result[json, RuntimeError]`

Example:

```gof
headers: dict[string] = {
    "Accept": "application/json",
    "Authorization": "Bearer token",
}
report = http_request("GET", "https://example.com/health", "", headers, 1500)?
status = json_int(json_get(report, "status")?)?
```

## Semantics

- the returned JSON report contains:
  - `status`
  - `body`
  - `headers`
  - `method`
  - `url`
- response header names are normalized to lowercase
- each response header value is exposed as `list[string]`
- non-success HTTP statuses remain successful report values so callers can
  branch on status explicitly
- transport-level failures still surface as `RuntimeError.HttpRequest(message)`
- timeout values must be non-negative integers in milliseconds

## TLS stance

This slice does not introduce a custom TLS stack.

HTTPS/TLS continues to ride on the bootstrap `ureq + rustls` transport path.
That keeps the network baseline aligned with existing bootstrap HTTP behavior
while improving request/response ergonomics.

## Alternatives considered

### Extend `http_post(...)` only

Rejected. That would still leave method choice, response headers, and structured
non-2xx handling underspecified.

### Collapse non-2xx into `RuntimeError.HttpStatus(...)`

Rejected for the new helper. Scripts that automate APIs often need to branch on
`404`, `409`, `429`, or `503` as normal control-flow data. The existing
`http_get(...)` / `http_post(...)` wrappers keep the body-only failure path for
simple use cases.

## Compatibility impact

- additive change only
- existing `http_get(...)` and `http_post(...)` behavior remains unchanged
- no new `RuntimeError` variant is introduced in this slice

## Diagnostics impact

- `GOF3005`: wrong number of arguments for `http_request(...)`
- `GOF3106`: invalid operand for structured bootstrap HTTP request helpers
- `GOF3107`: invalid timeout for structured bootstrap HTTP request helpers

## Out of scope

- HTTP server surface
- streaming request/response bodies
- websockets
- custom CA stores or certificate pinning
- HTTP/2 policy controls
- tracing/metrics integration

## Test plan

- typechecker coverage for valid and invalid `http_request(...)` calls
- interpreter coverage for:
  - structured success responses with headers
  - non-2xx status preservation
  - legacy `http_get(...)` / `http_post(...)` compatibility
- pipeline coverage for lower/type surface
- runnable example plus CLI smoke test
- docs, roadmap, and VS Code syntax/snippet sync
