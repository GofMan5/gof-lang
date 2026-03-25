# Concurrency

Concurrency is one of the reasons `gof` exists, so it needs a serious learning path from the start.

## `go` and `await`

```gof
fn square(x: int) -> int:
    return x * x

fn main() -> int:
    job = go square(12)
    return await job
```

`go` currently spawns a top-level named function call.

## Channels

```gof
fn main() -> int:
    ch: channel = channel()
    send(ch, 4)
    return recv(ch) + 1
```

Current channel baseline:

- `channel()` creates a bootstrap channel
- `send(channel, value)` transfers one value
- `recv(channel)` receives one value

## `select`

```gof
fn main() -> int:
    left: channel = channel()
    right: channel = channel()
    send(right, 8)
    select:
        value = recv(left):
            return 0
        value = recv(right):
            return value + 1
```

Current `select` contract:

- only receive arms are supported
- arms must be `recv(channel):` or `value = recv(channel):`
- the bootstrap runtime polls arms until one receive succeeds

## Current limits

This is a real baseline, but not the final concurrency story.

Still missing:

- cancellation
- close semantics
- fairness guarantees
- production scheduler hardening
- propagation rules for richer error models
