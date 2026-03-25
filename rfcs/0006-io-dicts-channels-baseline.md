# RFC 0006: bootstrap I/O, dicts, channels, and select baseline

## Summary

Extend the bootstrap `gof` subset with:

- `assert(condition[, message])`
- file I/O via `read_file(path)` and `write_file(path, contents)`
- dictionary values via `dict()` and `insert(dict, key, value)`
- typed indexing and membership checks for dicts
- channels via `channel()`, `send(channel, value)`, `recv(channel)`
- `select` over receive operations

## Motivation

The language already had structs, enums, methods, lists, and task spawning, but it still lacked:

- a built-in correctness contract for example programs
- real side effects beyond `print`
- a key/value aggregate type
- message-passing concurrency

Without these, `gof` stayed limited to pure compute demos.

## Design

### Assertions

- `assert(condition)` fails with a diagnostic when the condition is false
- `assert(condition, "message")` attaches a custom failure note
- return type: `unit`

### File I/O

- `read_file(path: string) -> string`
- `write_file(path: string, contents: string) -> unit`
- runtime errors remain explicit diagnostics

### Dict baseline

- `dict()` creates an empty dictionary
- keys are bootstrap strings
- `insert(dict, "key", value)` returns a new dict with the key updated
- `dict["key"]` indexes an existing key
- `contains(dict, "key")` checks key membership
- `len(dict)` returns entry count

### Channels and select

- `channel()` creates a bootstrap channel
- `send(channel, value)` sends one value
- `recv(channel)` receives one value
- `select` currently accepts only receive arms
- arm syntax:
  - `recv(ch):`
  - `value = recv(ch):`

## Non-goals

- dict literals
- typed channel payload syntax in source
- buffered channels
- send arms inside `select`
- closing channels
- networking or socket I/O
