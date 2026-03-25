# Types and Data

## Lists

```gof
values = [10, 20, 30]
return values[0] + len(values)
```

Lists are currently homogeneous once the bootstrap type layer can determine their element type.

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
- `insert(...)` returns a new dict
- indexing requires an existing key
- `contains(dict, "key")` checks key membership

## Structs

```gof
struct Point:
    x: int
    y: int

fn total(point: Point) -> int:
    return point.x + point.y
```

Structs are the main user-defined aggregate type in the current language subset.

## Enums

```gof
enum Status:
    Ready
    Busy
    Failed
```

Current enums use unit variants. Payload variants are future work.

## Methods

```gof
fn Point.with_bonus(self: Point, extra: int) -> int:
    return self.x + self.y + extra
```

Methods are explicit. There is no hidden receiver magic.
