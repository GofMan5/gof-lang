# Concurrency

Concurrency is one of the reasons `gof` exists, so it should be learned carefully.

The current implementation already exposes a real baseline, but it is still a
bootstrap baseline. That means you can write concurrent examples today, but you
should not confuse that with a finished production runtime contract.

## Step 1: `go` and `await`

```gof
fn square(x: int) -> int:
    return x * x

fn main() -> int:
    job = go square(12)
    return await job
```

Current rules:

- `go` currently spawns a top-level named function call
- the result is a task value
- `await` waits for that task value

The mental model is:

- `go` creates concurrent work
- `await` joins that work back into the current flow

## Step 2: channels

```gof
fn main() -> int:
    ch: channel = channel()
    send(ch, 4)
    return recv(ch) + 1
```

Current channel baseline:

- `channel()` creates a bootstrap channel
- `send(channel, value)` sends one value
- `recv(channel)` receives one value

Important honesty note:

> The current source language does not yet expose explicit `channel[T]` syntax.
> The runtime and type layer already know about channel values, but the public type
> surface for channel payloads is still intentionally narrow.

## Step 3: `select`

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

That is enough to model simple message-passing choices, which is already more honest
than adding pretty syntax with no execution model behind it.

## What is still missing

This is a real concurrency baseline, but not the final story.

Still missing:

- cancellation
- close semantics
- fairness guarantees
- richer propagation rules
- production scheduler hardening

## The right way to read current concurrency docs

Think of current `gof` concurrency as:

- real enough to learn from
- real enough to test
- not yet strong enough to promise production-grade semantics

That distinction matters because concurrency is one of the easiest places for a
language project to oversell itself.
