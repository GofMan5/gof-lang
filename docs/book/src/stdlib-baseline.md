# Standard Library Baseline

The current standard-library surface is deliberately small.

## Output and checks

```gof
print("ready")
assert(true, "must stay true")
```

`assert` is important during bootstrap because it gives examples and tests an in-language correctness tool.

## File I/O

```gof
fn main() -> int:
    path = "target/demo.txt"
    write_file(path, "gof")
    return len(read_file(path))
```

Current file I/O is text-only and intentionally direct.

## Collection helpers

```gof
values = [1, 2]
values = append(values, 3)
has_three = contains(values, 3)
```

```gof
mut store: dict = dict()
store = insert(store, "name", 1)
present = contains(store, "name")
```

## Philosophy

The standard library should grow only when the language contract is stable enough to support it well.

That means:

- no decorative surface area
- no helper that hides surprising allocations
- no fake convenience that blocks optimizer work later
