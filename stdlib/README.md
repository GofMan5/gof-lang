# stdlib

The standard library is intentionally separate from `core`.

Current first-wave shipped module names:

- `bytes`
- `io`
- `time`
- `net`
- `http`

These names are now reserved import targets. `import http` resolves to
`stdlib/http.gof`, not to a same-directory module or local dependency alias.

Current shipped foundation in this slice:

- `bytes`
  - `Bytes`
  - `bytes_from_string`, `bytes_to_string`, `bytes_len`, `bytes_slice`, `bytes_concat`
- `io`
  - `ReadStream`, `WriteStream`
  - `open_read_stream`, `open_read_stream_with_timeout`, `open_write_stream`, `open_write_stream_with_timeout`
  - explicit read/write/flush/close plus deadline/cancel/timeout wrappers
  - string helpers: `ReadStream.read_string`, `ReadStream.read_exact_string`, `ReadStream.read_all_string`, `WriteStream.write_string`, `WriteStream.write_all_string`
- `time`
  - `NetDeadline`
  - `deadline_after`, `deadline_at_unix_millis`
- `net`
  - `DuplexStream`, `TcpListener`, `SocketAddr`
  - `connect_tcp`, `connect_tcp_loopback`, `connect_tcp_with_control`, `connect_tcp_with_timeout`, `connect_tcp_with_timeout_budget`, `connect_tcp_loopback_with_control`, `connect_tcp_loopback_with_timeout`, `connect_tcp_loopback_with_timeout_budget`
  - `listen_tcp`, `listen_tcp_with_timeout`, `listen_tcp_loopback`, `listen_tcp_loopback_with_timeout`, `DuplexStream.with_timeout`, `TcpListener.with_timeout`, `TcpListener.accept_with_timeout`, `TcpListener.accept_with_timeout_budget`
  - string helpers: `DuplexStream.read_string`, `DuplexStream.read_exact_string`, `DuplexStream.read_all_string`, `DuplexStream.write_string`, `DuplexStream.write_all_string`
  - `SocketAddr.connect_tcp`, `SocketAddr.connect_tcp_with_control`, `SocketAddr.connect_tcp_with_timeout`, `SocketAddr.connect_tcp_with_timeout_budget`
- `http`
  - `request_json_headers`, `request_headers_set`, `request_headers_merge`, `request_json_headers_with`, `request_bearer_headers`, `request_bearer_headers_with`, `request_json_bearer_headers`, `request_json_bearer_headers_with`, `get_json`, `get_json_with_timeout`, `get_json_with_headers`, `post_json`, `post_json_with_timeout`, `post_json_with_headers`, `request_report`, `request_report_with_headers`, `get_report`, `get_report_with_headers`, `post_report`, `post_report_with_headers`, `request_json_report`, `request_json_report_with_headers`, `request_json_with_headers`
  - `response_status`, `response_status_class`, `response_is_success`, `response_require_success`
  - `response_body`, `response_json`, `response_json_success`, `response_method`, `response_url`
  - `response_headers`, `response_header_values`, `response_header`, `response_content_type`

The `http` module name is now shipped with explicit response-inspection helpers
on top of the existing bootstrap HTTP builtins. Broader typed client/server
surface still belongs to the next network slices.

It now also carries the common JSON-automation request path: default JSON
headers, composable header builders, parsed JSON GET/POST helpers, explicit
timeout variants, and a structured JSON request-report wrapper for service
clients that want the higher-level path before dropping to raw
`http_request(...)` control.

That same header-builder surface now also includes
`request_bearer_headers_with(token, extra)` so lower-level raw/report callers
can add bearer auth and explicit overrides without borrowing the JSON-specific
helper family.

It also exposes parsed-JSON custom-header helpers so authenticated JSON API
clients can keep the high-level `json` response path even when they need
bearer auth, trace headers, or other explicit request metadata.

That same parsed-JSON custom-header path now also includes dedicated GET/POST
wrappers so callers do not have to repeat the generic method/body placeholders
when the real contract is still just an authenticated JSON GET or POST.

The lower-level structured report path now mirrors that convenience with
generic `request_report...` wrappers for verbs like PATCH/DELETE plus dedicated
GET/POST wrappers for callers that want explicit response metadata inspection
without leaving the raw report contract.

That same `http` surface now also includes explicit success-gate helpers so the
higher-level parsed JSON paths raise `RuntimeError.HttpStatus(...)` on non-2xx
replies while the raw request-report helpers keep the structured inspection
surface intact.

That same surface now also includes declarative header-merge helpers so callers
can compose default JSON headers with overrides in one expression instead of
rebinding the same dict through repeated `request_headers_set(...)` steps.

The `net` module now also ships loopback bind/connect wrappers and typed
`SocketAddr` connection helpers so local service workers do not have to rebuild
`"127.0.0.1:" + to_string(port)` or `address.text()` manually.

That same `net` surface now also includes `TcpListener.accept_with_timeout`
and `TcpListener.accept_with_timeout_budget` so server code can arm the accept
wait and, when needed, the first returned `DuplexStream` timeout without an
immediate rebinding step after `accept()`.

The shipped `io` and `net` modules now also expose Result-returning timeout
wrappers that collapse the common `deadline_after(...)` plus `.with_deadline(...)`
pattern without hiding the fact that timeout setup itself can fail.

The `net` module now also exposes single-budget connect helpers so the common
client path can keep one explicit timeout value across connect establishment and
the first stream wrapper instead of rebinding `stream.with_timeout(...)`
immediately after a successful dial.

They now also expose timeout-armed open/listen constructors for the common
single-budget file and local-service path, so callers do not have to rebind a
fresh stream or listener just to apply the first timeout wrapper.

They also expose explicit UTF-8 stream helpers over the existing `Bytes`
foundation so callers can stay in text mode when that is the real contract
without pretending raw byte boundaries or decode failures do not exist.

Planned first-wave areas:

- collections
- io
- net
- time
- json
- test support

No surface in `stdlib/` should be treated as stable until it has spec text and conformance coverage.
