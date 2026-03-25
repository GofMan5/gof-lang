# Language Tour

This chapter gives the shortest honest overview of the current `gof` surface.

## Functions

```gof
fn add(a: int, b: int) -> int:
    return a + b
```

## Bindings

Bindings are immutable by default.

```gof
total = 10
```

Use `mut` only when reassignment is intentional.

```gof
mut total: int = 10
total = total + 1
```

## Builtin values and containers

`gof` currently supports:

- `int`
- `string`
- `bool`
- `list`
- `dict`
- `channel`
- `task`
- `unit`

## Builtin helpers

Today the bootstrap language surface includes:

- `len(...)`
- `print(...)`
- `assert(...)`
- `append(...)`
- `contains(...)`
- `read_file(...)`
- `write_file(...)`
- `dict()`
- `insert(...)`
- `channel()`
- `send(...)`
- `recv(...)`

These are intentionally small and explicit. The project is avoiding a fake-big standard library until the contracts are stable.
