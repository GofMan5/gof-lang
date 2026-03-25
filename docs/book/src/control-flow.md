# Control Flow

## `if` and `while`

```gof
fn main() -> int:
    mut n: int = 3
    mut total: int = 0
    while n > 0:
        if n > 1:
            total = total + n
        else:
            total = total + 10
        n = n - 1
    return total
```

Conditions must be boolean.

## `match`

`match` is currently statement-level and exhaustive over enum values.

```gof
enum Status:
    Ready
    Busy
    Failed

fn score(status: Status) -> int:
    match status:
        Status.Ready:
            return 100
        Status.Busy:
            return 50
        Status.Failed:
            return 0
```

Current rules:

- the target must be an enum
- every variant must be covered
- duplicate arms are rejected
- arms from a different enum are rejected
