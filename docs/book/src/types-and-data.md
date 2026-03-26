# Types and Data

This chapter explains how data is modeled in the current bootstrap subset and,
just as importantly, what is still intentionally missing.

## Builtin scalar values

Today the core scalar values are:

- `int`
- `string`
- `bool`

There is also `unit`, which represents "no meaningful value", and runtime-level
values such as tasks and channels that appear through specific language features.

## Lists

Lists are the first built-in sequential container.

```gof
values = [10, 20, 30]
return values[0] + len(values)
```

Current list rules:

- lists are currently homogeneous once the type layer can determine element type
- indexing requires an integer index
- negative indexing is not supported
- out-of-bounds access is rejected at runtime in the bootstrap evaluator
- `append(list, value)` returns a new list

The important teaching point is this:

> `append(...)` is explicit value construction, not invisible mutation.

## Dicts

The current dict baseline is explicit and immutable-friendly:

```gof
mut store: dict = dict()
store = insert(store, "ok", 7)
store = insert(store, "warn", 2)
return store["ok"] + len(store)
```

Current dict rules:

- keys are strings
- `dict()` creates an empty dict
- `insert(dict, key, value)` returns a new dict
- indexing requires an existing key
- `contains(dict, "key")` checks key membership
- `len(dict)` returns entry count

Current non-goals:

- no dict literals yet
- no non-string keys
- no user-visible generic dict syntax

## Structs

Structs are the current user-defined aggregate type.

```gof
struct Point:
    x: int
    y: int

fn total(point: Point) -> int:
    return point.x + point.y
```

Current struct rules:

- fields are declared in the type
- constructor calls use declaration order
- field access requires a real struct value
- unknown fields are rejected

Structs are how you model data that has named parts and stable shape.

## Enums

Enums are the current way to model finite state.

```gof
enum Status:
    Ready
    Busy
    Failed
```

Current enum rules:

- variants are unit variants only
- variant names must be unique inside the enum
- enum equality currently works only within the same enum type

Use `enum` when the question is "which state am I in?" rather than "which fields do I have?"

## Methods

Methods are explicit, not magical.

```gof
fn Point.with_bonus(self: Point, extra: int) -> int:
    return self.x + self.y + extra
```

Important rule:

- the receiver type is declared in the method name
- the first parameter must match that receiver type

This keeps the method model simple and visible during bootstrap.

## How to think about data in `gof`

A useful rule of thumb:

- use scalars for direct values
- use lists for ordered homogeneous data
- use dicts for string-keyed lookup tables
- use structs for named shaped data
- use enums for closed state sets

That distinction makes later control flow and concurrency code much clearer.
