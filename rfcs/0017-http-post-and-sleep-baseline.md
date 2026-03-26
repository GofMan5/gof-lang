# RFC 0017: HTTP POST and Explicit Sleep Baseline

## Summary

This RFC extends the bot-oriented bootstrap runtime slice with:

- `http_post(url, body)`
- `http_post(url, body, content_type)`
- `sleep(milliseconds)`

The goal is to move `gof` from "can poll bot updates" to "can both poll and
send bot responses through explicit, testable contracts."

## Design goals

- keep HTTP write paths explicit instead of smuggling side effects through query strings
- make retry and backoff intent visible in source programs
- preserve the existing `Result[..., RuntimeError]` operational error model
- avoid pretending the runtime already has a full async networking stack

## HTTP contract

`http_post(...)` returns `Result[string, RuntimeError]`.

Rules:

- argument 1 must be a string URL
- argument 2 must be a string request body
- argument 3, when present, must be a string content type
- when omitted, content type defaults to `text/plain; charset=utf-8`
- transport failures become `RuntimeError.HttpRequest(...)`
- non-success HTTP statuses become `RuntimeError.HttpStatus(code, body)`

Current non-goals:

- custom headers
- streaming request or response bodies
- multipart/form-data
- timeouts as part of the public surface

## Delay contract

`sleep(milliseconds)` returns `unit`.

Rules:

- the argument must be an `int`
- the value is interpreted as milliseconds
- negative values are rejected with a runtime diagnostic

The purpose of `sleep(...)` in the bootstrap surface is not "async sugar." It is
an explicit backoff primitive for CLI and bot retry loops.

## Teaching goal

After RFC 0016, `gof` could already fetch Telegram updates.

After this RFC, a baseline bot can:

- poll `getUpdates` with `http_get(...)`
- parse the payload
- answer through `http_post(...)`
- express a deliberate retry pause through `sleep(...)`

That is the minimum useful shape for a long-polling bot example without
inventing a framework first.
