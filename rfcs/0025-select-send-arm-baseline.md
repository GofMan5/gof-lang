# RFC 0025: `select` send-arm baseline

- Status: Accepted
- Area: concurrency, runtime, typechecker, docs
- Created: 2026-03-28

## Summary

Extend bootstrap `select` from receive-only operation arms to full operation
arms:

- receive arms
- send arms
- a single `default:` fallback arm

This keeps the existing blocking `send(...)`/`recv(...)` contracts intact while
making non-blocking channel backpressure paths expressible without helper
threads or fake probe channels.

## Motivation

After RFC 0023 and RFC 0024, `gof` already had:

- a real receive-based `select` baseline
- deterministic round-robin polling start rotation
- explicit channel capacities through `channel(capacity)`

What was still missing was the symmetric operational half of the model:
choosing whether a send can proceed right now.

Without send arms, bounded channels and rendezvous-style flows still force
awkward code:

- try a helper send elsewhere
- or move fallback logic outside the actual choice site

That hides intent and makes service-style concurrency less honest.

## Public surface

### Supported `select` arms

- `recv(channel):`
- `value = recv(channel):`
- `recv(channel, token):`
- `value = recv(channel, token):`
- `send(channel, value):`
- `value = send(channel, value):`
- `send(channel, value, token):`
- `value = send(channel, value, token):`
- one optional `default:`

### Binding contract

- receive-arm bindings keep the existing `Result[T, RuntimeError]` contract
- send-arm bindings expose `Result[unit, RuntimeError]`, matching plain
  `send(...)`

## Semantics

- `select` prepares each send/receive operation once at select-entry
- the runtime then polls those prepared operations until one becomes ready
- when a send arm wins, its body sees the same `Result[unit, RuntimeError]`
  value that a normal `send(...)` expression would produce
- `default:` still executes immediately when no send/receive arm is ready in
  the current polling pass
- the existing deterministic round-robin polling start rotation now applies to
  all prepared operation arms, not only receives

Preparing operations once is part of the contract for this bootstrap baseline:
arm expressions with side effects are not re-evaluated on every polling pass.

## Diagnostics

- `GOF3047`: invalid `select` contract or send/receive arm
- `GOF3095`: duplicate `default` arm in `select`

## Out of scope

- scheduler-level fairness guarantees beyond the current round-robin polling
  baseline
- richer rendezvous handoff semantics between independent `select` expressions
- deadline/context propagation beyond token-based cancellation
- final production-native memory-model guarantees

## Test plan

- parser coverage for send-arm syntax
- formatter coverage for send-arm rendering
- typed HIR coverage for send-arm lowering and binding type
- pipeline coverage for send-arm lowering through MIR/SSA
- interpreter coverage for:
  - ready send-arm execution
  - `default` fallback when a bounded send would block
- runnable example plus CLI execution coverage
