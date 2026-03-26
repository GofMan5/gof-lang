# RFC 0015: Result and Propagation Baseline

## Summary

This RFC adds a language-defined recoverable error baseline to `gof` through:

- builtin `Result[T, E]`
- builtin constructors `Result.Ok(value)` and `Result.Err(error)`
- exhaustive `match` over `Result`
- postfix propagation syntax `expr?`

The goal is to make recoverable control flow explicit without exceptions,
runtime magic, or hidden implicit conversions.

## Motivation

`gof` already had payload enums and exhaustive `match`, but it still lacked a
first-class contract for recoverable failures. That blocked later work on:

- file and process APIs
- environment access
- channel lifecycle errors
- task propagation
- Telegram-bot-grade HTTP and JSON flows

`Result[T, E]` closes that gap while reusing the same payload-variant mental
model as normal enums.

## Surface

```gof
fn halve(value: int) -> Result[int, string]:
    if value % 2 != 0:
        return Result.Err("odd")
    return Result.Ok(value / 2)

fn compute() -> Result[int, string]:
    half = halve(84)?
    return Result.Ok(half)
```

`match` works directly on `Result`:

```gof
match compute():
    Result.Ok(value):
        return value
    Result.Err(error):
        return len(error)
```

## Semantics

- `Result[T, E]` is a builtin parameterized type.
- `Result.Ok(value)` constructs a successful result payload.
- `Result.Err(error)` constructs an error result payload.
- `expr?` requires the operand to be `Result[T, E]`.
- `expr?` unwraps `Ok(value)` to `value`.
- `expr?` returns early from the enclosing function when the operand is
  `Err(error)`.
- The enclosing function must return a compatible `Result[_, E]`.
- `match` over `Result` is exhaustive only when both `Result.Ok(...)` and
  `Result.Err(...)` arms are present.

## Non-goals in this slice

- no exception system
- no implicit conversion from runtime diagnostics into `Result`
- no attempt to retrofit every bootstrap builtin to `Result` in the same change
- no user-defined generic declarations

Operational APIs such as filesystem, environment, or channel-close contracts
will migrate onto `Result` in later checkpoints.

## Diagnostics

- `GOF3067`: invalid type annotation arity for builtin parameterized types
- `GOF3071`: invalid payload binding arity in a `match` arm
- `GOF3072`: invalid builtin `Result` variant reference
- `GOF3073`: invalid payload arity for a builtin `Result` variant constructor
- `GOF3074`: invalid operand for postfix `?`
- `GOF3075`: postfix `?` used outside a compatible `Result` return contract

## Acceptance

- parser supports `Result[T, E]` and postfix `?`
- typed HIR supports `Result` constructors, propagation, and exhaustive result
  matching
- evaluator supports `Result.Ok`, `Result.Err`, and early return from `?`
- examples, spec, diagnostics, roadmap, and both books are updated together
