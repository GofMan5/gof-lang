# RFC 0013: Numeric Surface and Parameterized Builtin Types

## Summary

This RFC extends the bootstrap `gof` surface with:

- unary minus
- integer division and modulo
- parameterized builtin type annotations
- runtime-fail fixture coverage for evaluator-backed failures

The goal is to make the language surface strong enough for the next error-model
milestones without introducing full user-defined generics.

## Accepted surface

### Numeric expressions

`gof` now supports:

```gof
value = -count
ratio = total / size
rest = total % size
```

Rules:

- unary `-` works only on `int`
- `/` and `%` work only on `int`
- `/` and `%` reject zero on the right-hand side at runtime in the bootstrap evaluator

### Parameterized builtin type annotations

The bootstrap parser and type layer now accept:

```gof
list[int]
dict[int]
channel[int]
task[int]
```

Rules:

- `dict[T]` stays string-keyed and describes the value type
- parameterized annotations are builtin-only in this stage
- bare `list`, `dict`, `channel`, and `task` remain accepted for bootstrap compatibility
- wrong type-argument arity is rejected

## Why this shape

- It aligns the surface syntax with the type shapes already present in typed HIR.
- It avoids inventing special syntax later for `Result[T, E]`.
- It strengthens the language before payload enums and the real error model land.

## Non-goals

- no user-defined generic declarations
- no floating-point model
- no recoverable `Result`-based division-by-zero yet
- no optimizer or backend changes beyond carrying the new operators through MIR/SSA
