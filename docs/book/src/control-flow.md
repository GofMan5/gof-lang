# Control Flow

`gof` is trying to keep control flow obvious. The current bootstrap subset already
has enough control flow to write meaningful logic without pretending to support
every future feature.

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

Current rules:

- conditions must be boolean
- `if` and `while` use indentation-based blocks
- comparison operators and logical operators feed these conditions

That sounds simple, but it matters because the language is rejecting "truthy/falsy"
ambiguity on purpose.

## `and`, `or`, and `not`

Boolean logic is explicit and type-checked:

```gof
if ready and not failed:
    return 1
```

Current rules:

- `and` and `or` require boolean operands
- `not` requires a boolean operand
- `and` and `or` preserve short-circuit behavior

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

Why this matters:

> `match` is not just prettier branching. It is the current way `gof` makes state
> handling explicit and complete.

That is why exhaustiveness exists already, even while much of the ecosystem is still bootstrap-grade.

## When to use which form

Use:

- `if` when you are deciding based on boolean facts
- `while` when you are stepping through repeated work
- `match` when you are branching over a closed enum state

That division keeps control flow readable and keeps state transitions honest.
