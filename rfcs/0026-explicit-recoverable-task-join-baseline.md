# RFC 0026: explicit recoverable task joins

- Status: Accepted
- Area: concurrency, runtime, typechecker, docs
- Created: 2026-03-28

## Summary

Add `await_result(task)` as an explicit recoverable join builtin for bootstrap
concurrency.

`await_result(task)` returns `Result[T, RuntimeError]` for any `task[T]`:

- `Result.Ok(value)` when the task completes successfully
- `Result.Err(RuntimeError.TaskFailed(...))` when the task ends with evaluator
  diagnostics before producing a value
- `Result.Err(RuntimeError.TaskPanicked(...))` when the task panics

This is additive. Existing `await task` semantics do not change.

## Motivation

The language already preserved task-boundary failures at `await` when the
spawned function explicitly returned `Result[..., RuntimeError]`. That was
useful, but it still left plain `task[T]` joins in an awkward place:

- `await task` for plain tasks still surfaced diagnostics directly
- callers that wanted recoverable joins had to change the spawned function
  contract instead of the join site

That made propagation policy too implicit.

`await_result(task)` fixes that by moving the decision to the join site:

- plain `await` stays strict
- `await_result(task)` is the explicit recoverable path

## Public surface

- builtin: `await_result(task)`
- accepted operand: exactly one task value produced by `go`
- return type: `Result[T, RuntimeError]` for `task[T]`

## Semantics

- successful completion becomes `Result.Ok(task_value)`
- evaluator diagnostics become
  `Result.Err(RuntimeError.TaskFailed(message: ...))`
- task panics become
  `Result.Err(RuntimeError.TaskPanicked(task: ...))`
- if the task value itself is `task[Result[U, E]]`, the builtin does not
  flatten; the result is `Result[Result[U, E], RuntimeError]`

This keeps the bootstrap contract predictable and avoids hidden policy at the
join boundary.

## Diagnostics

- `GOF3005`: wrong number of arguments for `await_result`
- `GOF3009`: non-task operand passed to `await_result`

## Out of scope

- changing the meaning of plain `await task`
- automatic flattening of nested `Result` task payloads
- scheduler-level fairness or structured-cancellation completion
- final native runtime guarantees

## Test plan

- interpreter coverage for:
  - successful plain-task recoverable join
  - diagnostics-to-`RuntimeError.TaskFailed(...)` conversion
  - panic-to-`RuntimeError.TaskPanicked(...)` conversion
  - invalid operand rejection
- typechecker coverage for inferred `Result[T, RuntimeError]` return type
- pipeline coverage for compile-through of the builtin
- runnable example plus CLI execution coverage
