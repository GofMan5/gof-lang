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
  - `open_read_stream`, `open_write_stream`
  - explicit read/write/flush/close plus deadline/cancel/timeout wrappers
  - string helpers: `ReadStream.read_string`, `ReadStream.read_exact_string`, `ReadStream.read_all_string`, `WriteStream.write_string`, `WriteStream.write_all_string`
- `time`
  - `NetDeadline`
  - `deadline_after`, `deadline_at_unix_millis`
- `net`
  - `DuplexStream`, `TcpListener`, `SocketAddr`
  - `connect_tcp`, `connect_tcp_loopback`, `connect_tcp_with_control`, `connect_tcp_with_timeout`, `connect_tcp_loopback_with_control`, `connect_tcp_loopback_with_timeout`
  - `listen_tcp`, `listen_tcp_loopback`, `DuplexStream.with_timeout`, `TcpListener.with_timeout`
  - string helpers: `DuplexStream.read_string`, `DuplexStream.read_exact_string`, `DuplexStream.read_all_string`, `DuplexStream.write_string`, `DuplexStream.write_all_string`
  - `SocketAddr.connect_tcp`, `SocketAddr.connect_tcp_with_control`, `SocketAddr.connect_tcp_with_timeout`
- `http`
  - `response_status`, `response_status_class`, `response_is_success`
  - `response_body`, `response_json`, `response_method`, `response_url`
  - `response_headers`, `response_header_values`, `response_header`, `response_content_type`

The `http` module name is now shipped with explicit response-inspection helpers
on top of the existing bootstrap HTTP builtins. Broader typed client/server
surface still belongs to the next network slices.

The `net` module now also ships loopback bind/connect wrappers and typed
`SocketAddr` connection helpers so local service workers do not have to rebuild
`"127.0.0.1:" + to_string(port)` or `address.text()` manually.

The shipped `io` and `net` modules now also expose Result-returning timeout
wrappers that collapse the common `deadline_after(...)` plus `.with_deadline(...)`
pattern without hiding the fact that timeout setup itself can fail.

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
