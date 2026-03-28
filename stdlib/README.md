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
  - explicit read/write/flush/close plus deadline/cancel wrappers
- `time`
  - `NetDeadline`
  - `deadline_after`, `deadline_at_unix_millis`
- `net`
  - `DuplexStream`, `TcpListener`, `SocketAddr`
  - `connect_tcp`, `connect_tcp_with_control`, `listen_tcp`

The `http` module name is reserved and shipped, but its typed client/server
surface still belongs to the next network slices. For now the public HTTP
contract is still the bootstrap helper layer documented in `README.md` and
`spec/language-v1.md`.

Planned first-wave areas:

- collections
- io
- net
- time
- json
- test support

No surface in `stdlib/` should be treated as stable until it has spec text and conformance coverage.
