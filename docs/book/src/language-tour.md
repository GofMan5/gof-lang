# Language Tour

This chapter is the shortest honest overview of the current `gof` surface.

It is not trying to teach every edge case. It is trying to make the language feel
mentally coherent before you dive deeper.

## The core idea

`gof` code should read simply, but its behavior should stay explicit.

That means the language is currently built around:

- top-level functions
- clear type contracts
- immutable bindings by default
- visible mutation through `mut`
- explicit data modeling with `struct` and `enum`
- explicit loop control through `break` and `continue`
- explicit concurrency with tasks, channels, and `select`

## Functions are the main execution unit

```gof
fn add(a: int, b: int) -> int:
    return a + b
```

Current function rules:

- functions live at top level
- parameters can carry type annotations
- return types can be declared explicitly
- return paths must stay type-compatible

At the current bootstrap stage, plain calls target top-level named functions.

## Bindings are immutable unless you say otherwise

```gof
total = 10
```

That creates an immutable binding.

If reassignment is part of the logic, you must opt into it:

```gof
mut total: int = 10
total = total + 1
```

This rule is central to how `gof` is being taught. Mutation is allowed, but it is
not the silent default.

## Values you can work with today

The current bootstrap subset already supports:

- `int`
- `string`
- `bool`
- `json`
- lists
- dicts
- user-defined structs
- user-defined enums
- `Result[T, E]`
- task values
- channel values
- cancellation-token values
- `unit`

That is enough to write non-trivial examples, but not enough to pretend the full
language ecosystem already exists.

## Builtin helpers are intentionally small

Today the bootstrap language surface includes:

- `len(...)`
- `print(...)`
- `assert(...)`
- `append(...)`
- `contains(...)`
- `trim(...)`
- `split(...)`
- `join(...)`
- `starts_with(...)`
- `ends_with(...)`
- `parse_int(...)`
- `to_string(...)`
- `range(...)`
- `sleep(...)`
- `argv()`
- `read_stdin()`
- `read_stdin_lines()`
- `unix_seconds()`
- `unix_millis()`
- `env(...)`
- `cwd()`
- `run_process(...)`
- `read_file(...)`
- `write_file(...)`
- `exists(...)`
- `read_dir(...)`
- `mkdir(...)`
- `remove_file(...)`
- `path_join(...)`
- `path_dir(...)`
- `path_base(...)`
- `path_ext(...)`
- `dict()`
- `insert(...)`
- `keys(...)`
- `values(...)`
- `channel()`
- `close(...)`
- `send(...)`
- `recv(...)`
- `cancel_token()`
- `cancel(...)`
- `is_cancelled(...)`
- `json_parse(...)`
- `json_stringify(...)`
- `json_get(...)`
- `json_index(...)`
- `json_len(...)`
- `json_string(...)`
- `json_int(...)`
- `template_render(...)`
- `http_get(...)`
- `http_post(...)`

The small size is deliberate.

> `gof` would rather have a small standard-library surface with explicit behavior
> than a large surface that teaches the wrong semantics.

The same rule applies to data literals. Lists and dicts should be readable
without pretending that allocation or mutation disappeared.

## A tiny program that already feels like `gof`

```gof
struct Point:
    x: int
    y: int

fn Point.total(self: Point, extra: int) -> int:
    return self.x + self.y + extra

fn main() -> int:
    point: Point = Point(3, 4)
    return point.total(5)
```

This one example already shows the current language shape:

- typed functions
- structs
- methods
- explicit receiver contract
- explicit return value

That combination is closer to what `gof` wants to be than any single syntax feature.

At the current stage, that same design style already extends to operational code:

- side effects stay explicit through helper calls
- recoverable failures flow through `Result`
- concurrency stays message-passing-first
- bot-oriented programs can be expressed without pretending a web framework already exists
