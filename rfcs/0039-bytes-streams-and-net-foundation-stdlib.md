# RFC 0039: bytes, streams, deadlines, and raw TCP foundation through shipped stdlib

## Summary

Introduce the first real network/runtime foundation through shipped stdlib
modules instead of through permanent global builtins.

This slice adds:

- `bytes` module with opaque `Bytes`
- `io` module with `ReadStream` and `WriteStream`
- `time` module with opaque `NetDeadline`
- `net` module with `DuplexStream`, `TcpListener`, and `SocketAddr`
- explicit `Result`-based contracts for file streams, TCP, and deadline-aware blocking

## Motivation

`gof` cannot grow into a serious service/runtime language by stacking more
global helper builtins forever.

The project already has reserved shipped stdlib import names. The next step is
to use those imports for the first typed bytes/stream/network layer so future
HTTP, TLS, proxy, and observability work can build on real types instead of on
JSON reports and ad hoc string helpers.

## Public surface

### `bytes`

- `Bytes`
- `bytes_from_string(text) -> Bytes`
- `bytes_to_string(bytes) -> Result[string, RuntimeError]`
- `bytes_len(bytes) -> int`
- `bytes_slice(bytes, start, end) -> Result[Bytes, RuntimeError]`
- `bytes_concat(left, right) -> Bytes`

Methods:

- `Bytes.len() -> int`
- `Bytes.slice(start, end) -> Result[Bytes, RuntimeError]`
- `Bytes.to_string() -> Result[string, RuntimeError]`

### `io`

- `open_read_stream(path) -> Result[ReadStream, RuntimeError]`
- `open_write_stream(path) -> Result[WriteStream, RuntimeError]`

Methods:

- `ReadStream.read(max_bytes) -> Result[Bytes, RuntimeError]`
- `ReadStream.read_exact(bytes) -> Result[Bytes, RuntimeError]`
- `ReadStream.read_all() -> Result[Bytes, RuntimeError]`
- `ReadStream.close() -> Result[unit, RuntimeError]`
- `ReadStream.with_deadline(deadline) -> ReadStream`
- `ReadStream.with_cancel(token) -> ReadStream`
- `WriteStream.write(bytes) -> Result[int, RuntimeError]`
- `WriteStream.write_all(bytes) -> Result[int, RuntimeError]`
- `WriteStream.flush() -> Result[unit, RuntimeError]`
- `WriteStream.close() -> Result[unit, RuntimeError]`
- `WriteStream.with_deadline(deadline) -> WriteStream`
- `WriteStream.with_cancel(token) -> WriteStream`

### `time`

- `deadline_after(milliseconds) -> Result[NetDeadline, RuntimeError]`
- `deadline_at_unix_millis(unix_millis) -> Result[NetDeadline, RuntimeError]`

Methods:

- `NetDeadline.unix_millis() -> int`
- `NetDeadline.remaining_millis() -> int`

### `net`

- `connect_tcp(address) -> Result[DuplexStream, RuntimeError]`
- `connect_tcp_with_control(address, deadline, token) -> Result[DuplexStream, RuntimeError]`
- `listen_tcp(address) -> Result[TcpListener, RuntimeError]`

Methods:

- `DuplexStream.read(max_bytes) -> Result[Bytes, RuntimeError]`
- `DuplexStream.read_exact(bytes) -> Result[Bytes, RuntimeError]`
- `DuplexStream.read_all() -> Result[Bytes, RuntimeError]`
- `DuplexStream.write(bytes) -> Result[int, RuntimeError]`
- `DuplexStream.write_all(bytes) -> Result[int, RuntimeError]`
- `DuplexStream.flush() -> Result[unit, RuntimeError]`
- `DuplexStream.close() -> Result[unit, RuntimeError]`
- `DuplexStream.with_deadline(deadline) -> DuplexStream`
- `DuplexStream.with_cancel(token) -> DuplexStream`
- `DuplexStream.peer_addr() -> Result[SocketAddr, RuntimeError]`
- `DuplexStream.local_addr() -> Result[SocketAddr, RuntimeError]`
- `TcpListener.accept() -> Result[DuplexStream, RuntimeError]`
- `TcpListener.close() -> Result[unit, RuntimeError]`
- `TcpListener.with_deadline(deadline) -> TcpListener`
- `TcpListener.with_cancel(token) -> TcpListener`
- `TcpListener.local_addr() -> Result[SocketAddr, RuntimeError]`
- `SocketAddr.text() -> string`
- `SocketAddr.port() -> int`

## Error taxonomy

This slice extends `RuntimeError` with:

- `Utf8(message: string)`
- `NetDns(message: string)`
- `NetConnect(message: string)`
- `NetTimeout(message: string)`
- `NetTls(message: string)`
- `NetProxy(message: string)`
- `NetProtocol(message: string)`
- `NetReset(message: string)`
- `NetClosed`

The goal is to keep transport/protocol failures explicit early, before typed
HTTP and server layers are introduced.

## Design notes

- Public API lives in shipped stdlib modules, not as new permanent global builtins.
- Runtime values behind this surface are opaque so the bootstrap language does
  not pretend to expose layout or ownership guarantees it does not yet have.
- Deadline and cancellation control are explicit wrappers on stream/listener
  values. Blocking behavior must never hide behind ambient global state.
- `http`, TLS, proxy configuration, and observability remain follow-up slices.

## Out of scope

This RFC does not add:

- typed HTTP client/server surface
- TLS or mTLS configuration
- proxy configuration
- HTTP/2, websocket, SSE, or QUIC
- zero-copy promises or stable binary layout guarantees
- incremental or evented runtime APIs exposed directly to user code
