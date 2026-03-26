# RFC 0016: Runtime Operations, JSON, HTTP, and Telegram Bot Baseline

## Summary

This RFC extends the bootstrap `gof` surface so the language can run a real
long-polling Telegram bot without inventing a web framework first.

The baseline adds:

- `argv()`
- `env(name)`
- `cwd()`
- `exists(path)`
- `read_dir(path)`
- `mkdir(path)`
- `remove_file(path)`
- `path_join(left, right)`
- `path_dir(path)`
- `path_base(path)`
- `path_ext(path)`
- `close(channel)`
- `cancel_token()`
- `cancel(token)`
- `is_cancelled(token)`
- `json_parse(text)`
- `json_stringify(value)`
- `json_get(value, key)`
- `json_index(value, index)`
- `json_len(value)`
- `json_string(value)`
- `json_int(value)`
- `http_get(url)`

This RFC also migrates `read_file(...)`, `write_file(...)`, `send(...)`, and
`recv(...)` onto explicit `Result[_, RuntimeError]` contracts.

## Design goals

- make recoverable operational failures explicit
- keep path and process helpers string-based in v1
- make channel lifecycle honest before promising production concurrency
- enable JSON and HTTP in a form that is good enough for bots and CLI tools
- avoid silent coercions or hidden fallback behavior

## Error model

Operational helpers return `Result[..., RuntimeError]`.

`RuntimeError` is a builtin enum with these variants:

- `EnvMissing(name: string)`
- `Io(message: string)`
- `ChannelClosed`
- `Cancelled`
- `Json(message: string)`
- `HttpRequest(message: string)`
- `HttpStatus(code: int, body: string)`

This keeps `?` usable across env, filesystem, channel, JSON, and HTTP flows
without forcing ad hoc conversion rules in the bootstrap language.

## Concurrency contract

The bootstrap concurrency surface now includes:

- `close(channel)`
- `send(channel, value)` and `send(channel, value, token)`
- `recv(channel)` and `recv(channel, token)`
- `cancel_token()`
- `cancel(token)`
- `is_cancelled(token)`

`send` and `recv` return `Result` values so close and cancellation are explicit.

Current non-goals:

- fairness guarantees
- structured cancellation trees
- panic propagation semantics beyond the existing task baseline

## JSON and HTTP contract

`json` is a builtin runtime value class. The bootstrap evaluator supports:

- null
- bool
- integer numbers
- strings
- arrays
- objects

`http_get(url)` returns response text on success and operational `RuntimeError`
variants on request or HTTP-status failure.

Current non-goals:

- HTTP POST or streaming
- JSON number precision beyond integers
- schema-driven decoding
- bot framework abstractions

## Teaching goal

The motivating example for this RFC is a long-polling Telegram bot:

- read token from `env("TELEGRAM_BOT_TOKEN")?`
- call `getUpdates`
- parse JSON
- route `/ping`
- answer with `sendMessage`

If the bootstrap language cannot do this cleanly, it is not yet ready for real
backend-style CLI automation work.
