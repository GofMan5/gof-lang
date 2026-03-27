# RFC 0023: `select default` baseline

## Summary

Add a single `default:` arm to bootstrap `select` so `gof` can express
non-blocking fallback paths without inventing fake readiness channels or busy
helper tasks.

## Motivation

Current `select` already has real receive semantics, typed channels, and a
deterministic round-robin polling baseline when multiple receive arms are
ready. What it still lacks is the canonical operational fallback shape that
real worker and service loops need:

- check channels if something is ready
- otherwise do immediate fallback work

Without `default:` the language forces awkward workarounds that hide intent.

## Proposed semantics

- a `select` arm may be either a receive arm or `default:`
- receive arms keep the existing forms:
  - `recv(channel):`
  - `value = recv(channel):`
  - `recv(channel, token):`
  - `value = recv(channel, token):`
- at most one `default:` arm is allowed in a given `select`
- if one or more receive arms are ready, the runtime chooses among ready receive
  arms using the existing deterministic round-robin polling baseline
- if no receive arm is ready during the current polling pass and a `default:`
  arm exists, the runtime executes `default:` immediately
- if no receive arm is ready and no `default:` arm exists, the current blocking
  polling behavior remains unchanged

## Diagnostics

- `GOF3095`: duplicate `default` arm in `select`

## Non-goals

- no send-arms in `select` yet
- no scheduler-level fairness claims beyond the current round-robin receive
  baseline
- no deadline/context propagation changes

## Test plan

- parser coverage for `default:`
- formatter coverage for `default:`
- typed HIR coverage for valid and duplicate-default `select`
- interpreter coverage for:
  - immediate `default` fallback
  - ready receive beating `default`
  - duplicate `default` rejection
- pipeline and CLI example coverage
