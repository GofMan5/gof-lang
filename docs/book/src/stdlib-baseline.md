# Standard Library Baseline

The current standard-library surface is deliberately small.

That is not because the language wants to stay tiny. It is because the project
does not want to teach unstable semantics as if they were settled.

## Output and correctness checks

```gof
print("ready")
assert(true, "must stay true")
```

`print(...)` is the current explicit output primitive.

`assert(...)` is important during bootstrap because it gives examples and tests an
in-language correctness tool without pretending a full testing framework already exists.

Current `assert` rules:

- `assert(condition)` checks a boolean
- `assert(condition, "message")` adds a message
- failed assertions become diagnostics in the bootstrap runtime

## File I/O

```gof
fn main() -> int:
    path = "target/demo.txt"
    write_file(path, "gof")
    return len(read_file(path))
```

Current file I/O is intentionally direct:

- `read_file(path)` returns a string
- `write_file(path, contents)` writes a string

This is not pretending to be a complete I/O library. It is a minimal baseline for
real side effects.

## Collection helpers

For lists:

```gof
values = [1, 2]
values = append(values, 3)
has_three = contains(values, 3)
```

For dicts:

```gof
mut store: dict = dict()
store = insert(store, "name", 1)
present = contains(store, "name")
```

These helpers are teaching an important `gof` habit:

- helper calls should be explicit
- data-shape changes should be visible
- the language should avoid surprising hidden work

## Why the stdlib is intentionally narrow

The standard library should grow only when the language contract is stable enough
to support it well.

That means:

- no decorative surface area
- no helper that hides surprising allocations
- no fake convenience that blocks optimizer work later
- no pretending the ecosystem is bigger than it is

The current stdlib is small, but it already teaches the right direction: explicit
helpers, explicit side effects, and explicit data-shape operations.
