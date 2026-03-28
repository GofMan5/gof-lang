# RFC 0027: cancellable `await_result` joins

- Status: Accepted
- Area: concurrency, runtime, typechecker, docs
- Created: 2026-03-28

## Summary

Extend `await_result(task)` to support an optional cancellation token:

- `await_result(task)`
- `await_result(task, token)`

The second form keeps the same `Result[T, RuntimeError]` return contract but
also allows the join-site to end with `Result.Err(RuntimeError.Cancelled)` if
the token is cancelled before the task produces an outcome.

## Motivation

After RFC 0026, `gof` already had an explicit recoverable join path for plain
tasks. What was still missing was a cancellation story for that join itself.

Channels already had:

- `recv(channel, token)`
- `send(channel, value, token)`
- `timeout_token(...)`
- `cancel_after(...)`

Task joins should be able to participate in the same bootstrap cancellation
surface. Otherwise a service-style caller can cancel a blocked receive, but not
cancel a blocked task join without rewriting the callee contract.

## Public surface

- `await_result(task)` keeps existing behavior
- `await_result(task, token)` adds an optional cancellation token
- accepted token type: `cancel_token`
- return type remains `Result[T, RuntimeError]`

## Semantics

- if the task finishes first:
  - success becomes `Result.Ok(value)`
  - diagnostics become `Result.Err(RuntimeError.TaskFailed(...))`
  - panics become `Result.Err(RuntimeError.TaskPanicked(...))`
- if the token is cancelled before the task finishes:
  - the join returns `Result.Err(RuntimeError.Cancelled)`
- task completion wins over cancellation when the task outcome is already
  available at the moment the join checks readiness

This is still a bootstrap polling baseline, not a final scheduler-integrated
structured-concurrency model.

## Diagnostics

- `GOF3005`: wrong number of arguments for `await_result`
- `GOF3009`: non-task first argument
- `GOF3082`: invalid optional cancellation-token argument

## Out of scope

- changing plain `await`
- task abortion or forced task termination
- scheduler-level deadlines and context propagation
- final structured-concurrency semantics

## Test plan

- interpreter coverage for:
  - successful join with a live token
  - cancelled join through `timeout_token(0)`
  - invalid optional token rejection
- typechecker coverage for two-argument typing and diagnostics
- pipeline coverage for compile-through of the two-argument form
- runnable example plus CLI coverage
