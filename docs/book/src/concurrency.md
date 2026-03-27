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
    job: task[int] = go square(12)
    return await job
```

Current rules:

- `go` currently spawns a top-level named function call
- the result is a task value
- `await` waits for that task value
- when the spawned function explicitly declares `-> Result[..., RuntimeError]`,
  task-boundary evaluator failures now come back as
  `Result.Err(RuntimeError.TaskFailed(...))` or
  `Result.Err(RuntimeError.TaskPanicked(...))` instead of tearing down the
  caller immediately

The mental model is:

- `go` creates concurrent work
- `await` joins that work back into the current flow

## Step 2: channels and explicit lifecycle

```gof
fn main() -> Result[int, RuntimeError]:
    ch: channel[int] = channel()
    send(ch, 4)?
    return recv(ch)
```

Current channel baseline:

- `channel[T]` is a real parameterized builtin type annotation
- `channel()` creates a bootstrap channel
- `close(channel)` closes it explicitly
- `send(channel, value)` returns `Result[unit, RuntimeError]`
- `recv(channel)` returns `Result[T, RuntimeError]`

With channel lifecycle, the bootstrap runtime already distinguishes:

- successful send/receive
- closed channel
- cancelled wait

## Step 3: `select`

```gof
fn main() -> Result[int, RuntimeError]:
    left: channel[int] = channel()
    right: channel[int] = channel()
    send(right, 8)?
    select:
        value = recv(left):
            return Result.Ok(value? + 0)
        value = recv(right):
            return Result.Ok(value? + 1)
```

Current `select` contract:

- only receive arms are supported
- arms must be `recv(channel):`, `value = recv(channel):`, `recv(channel, token):`, or `value = recv(channel, token):`
- the bootstrap runtime polls arms until one receive resolves to `Result.Ok(...)` or `Result.Err(...)`
- when multiple receive arms are already ready, the bootstrap runtime rotates the polling start arm in a deterministic round-robin baseline so the first source arm does not always win

That is enough to model simple message-passing choices, which is already more honest
than adding pretty syntax with no execution model behind it.

## Step 4: cancellation

```gof
fn main() -> bool:
    token = cancel_token()
    cancel(token)
    return is_cancelled(token)
```

Current cancellation baseline:

- `cancel_token()` creates a cooperative token
- `cancel(token)` marks the token as cancelled
- `is_cancelled(token)` reports the current state
- `send(..., token)` and `recv(..., token)` observe that token while blocking

## What is still missing

This is a real concurrency baseline, but not the final story.

Still missing:

- production-grade fairness guarantees beyond the current round-robin polling baseline
- task panic and `Result` propagation hardening for plain `task[T]` joins
- production scheduler hardening

## The right way to read current concurrency docs

Think of current `gof` concurrency as:

- real enough to learn from
- real enough to test
- not yet strong enough to promise production-grade semantics

That distinction matters because concurrency is one of the easiest places for a
language project to oversell itself.
