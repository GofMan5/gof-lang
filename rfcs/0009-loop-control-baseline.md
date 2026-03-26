# RFC 0009: Loop Control Baseline

## Summary

Add bootstrap loop-control statements:

```gof
for value in values:
    if value == 2:
        continue
    if value > 10:
        break
```

`break` exits the nearest loop. `continue` skips to the next iteration of the
nearest loop.

## Motivation

The language already supports `while` and `for`, but without loop-control
statements many normal algorithms become awkward:

- early exit requires extra boolean flags
- skipping one iteration requires inverted nesting
- examples teach clumsy patterns instead of honest control flow

That hurts readability and makes the bootstrap subset look less capable than it
really should.

## Design

- `break` is a statement
- `continue` is a statement
- both target the nearest enclosing `while` or `for`
- both lower through AST, HIR, typed HIR, MIR, SSA, and the evaluator
- both are rejected outside loops

## Non-goals

- labeled breaks
- multi-level break or continue
- loop `else`
- `break value` semantics

## Diagnostics

- `GOF3053`: invalid `break` outside a loop
- `GOF3054`: invalid `continue` outside a loop

## Teaching impact

The book and examples should teach `break` and `continue` as explicit control
flow, not as substitutes for structured state modeling. They belong in loops,
not in general branching logic.
