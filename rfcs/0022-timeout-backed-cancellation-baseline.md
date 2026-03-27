# RFC 0022: Timeout-Backed Cancellation Baseline

## problem

The cancellation surface was still too manual for realistic blocking workflows:

- `cancel_token()` created a token, but every timeout path had to be wired by hand
- blocked `recv(..., token)` and `send(..., token)` could observe cancellation, but there was no first-class timeout helper
- the concurrency story still lacked a small, honest bridge toward deadline-aware operational code

That left M5 cancellation useful, but still weaker than it needed to be for service-style retry, polling, and bounded wait flows.

## proposed change

Add two timeout-backed cancellation helpers:

- `timeout_token(milliseconds)` -> `cancel_token`
- `cancel_after(token, milliseconds)` -> `unit`

### semantic contract

- both helpers require non-negative millisecond durations
- `timeout_token(milliseconds)` returns a fresh token and schedules its cancellation after the requested delay
- `cancel_after(token, milliseconds)` schedules cancellation on an existing token
- `0` is valid and cancels immediately
- blocked `send(..., token)` and `recv(..., token)` continue to surface timeout-driven cancellation as `Result.Err(RuntimeError.Cancelled)`

### stage contract

This is a timeout-backed token baseline, not a finished context/deadline framework:

- there is still no request-scoped context propagation
- there is still no structured deadline tree
- there is still no graceful service shutdown contract

## alternatives considered

- forcing users to emulate timeouts with ad hoc worker tasks and manual `cancel(...)`
- adding full context/deadline propagation before the bootstrap runtime was ready
- overloading `sleep(...)` into a cancellation API instead of keeping timeout tokens explicit

## compatibility impact

- additive only
- no existing syntax or runtime contract changes
- existing manual cancellation code keeps working unchanged

## diagnostics impact

Adds two diagnostics:

- `GOF3093` for invalid timeout helper operands
- `GOF3094` for negative timeout helper durations

Existing cancellation-token type errors continue to use `GOF3082`.

## test plan

- interpreter tests for immediate timeout cancellation and scheduled cancellation through blocked `recv`
- typed HIR tests for builtin typing and timeout argument validation
- pipeline coverage for builtin lowering
- CLI conformance coverage through a runnable example

## benchmark impact

- no new benchmark gate in this slice
- timer-thread overhead remains bootstrap-only and must be revisited before production scheduler claims
