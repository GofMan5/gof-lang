# Testing Platform 04: Property, Fuzz, Stress, and Bench

## Goal

Make the testing platform strong enough for parser/compiler hardening,
concurrency validation, and performance regression gates.

## Contract

Planned language forms:

- `property fn`
- `fuzz fn`
- `stress fn`
- `bench fn`

Planned runner contracts:

- deterministic seeds for property and stress runs
- shrinking for core data domains
- crash corpus storage and minimization for fuzzing
- scheduler perturbation and trace capture for concurrency stress
- benchmark warmup plus median/p95 export

## Non-goals

- non-reproducible random testing
- hidden benchmark baselines mixed into snapshot output
- concurrency stress that depends on uncontrolled global state

## Diagnostics Impact

- property, fuzz, stress, and bench failures must report the seed or minimized
  repro input as part of the failure contract

## Tests

- generator/shrinker correctness for core types
- corpus read/write/minimize flow
- deterministic stress replay from captured seed and schedule trace
- stable benchmark JSON export for CI comparison

## Exit Criteria

- `gof` can validate not only semantics but robustness, concurrency behavior,
  and performance through the same testing platform
