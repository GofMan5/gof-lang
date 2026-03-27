# RFC 0024: Capacity-Aware Channel Baseline

- Status: Accepted
- Area: concurrency, runtime, typechecker, docs
- Created: 2026-03-28

## Summary

Extend the bootstrap channel surface from `channel()` only to:

- `channel()`
- `channel(capacity)`

This RFC keeps `channel()` as the existing unbounded bootstrap queue path for
backward compatibility and adds an explicit capacity baseline:

- `channel(0)` for rendezvous-style send/receive handoff
- `channel(n)` for bounded buffering when `n > 0`

## Motivation

The previous channel surface was typed and usable, but it did not provide an
honest backpressure story. `send(..., token)` already existed, yet the default
queue-backed path could not block on buffer pressure because there was no
explicit capacity contract.

This RFC closes that semantic gap without breaking existing examples or package
code that already rely on `channel()`.

## Public surface

### Builtins

- `channel()` keeps the existing bootstrap behavior
- `channel(capacity)` accepts one non-negative integer argument

### Diagnostics

- `GOF3096`: invalid operand for explicit channel capacity
- `GOF3097`: invalid negative channel capacity

## Semantics

### `channel()`

- Creates an unbounded bootstrap channel backed by the runtime queue model.
- Existing `send(...)`, `recv(...)`, `close(...)`, and `select` behavior remains
  source-compatible.

### `channel(0)`

- Creates a rendezvous channel baseline.
- `send(...)` blocks until a receiver takes the value.
- `recv(...)` blocks until a sender hands off a value.

### `channel(n)` where `n > 0`

- Creates a bounded buffered channel baseline.
- `send(...)` blocks while the buffer is full.
- `recv(...)` wakes blocked senders when it frees buffer space.

### Cancellation and close semantics

- `send(channel, value, token)` and `recv(channel, token)` continue to use the
  cooperative cancellation token contract while blocked.
- `close(channel)` wakes blocked senders and receivers.
- `recv(...)` still drains any queued values before reporting
  `RuntimeError.ChannelClosed`.

## Type-system impact

- No new public type is introduced.
- `channel[T]` remains the only channel type annotation.
- The capacity argument changes runtime behavior only; it does not alter the
  surface type.

## Out of scope

- send-arms inside `select`
- scheduler-level fairness guarantees beyond the existing round-robin polling
  baseline
- richer buffering policies or named channel constructors
- final memory-model guarantees for production-native backends
