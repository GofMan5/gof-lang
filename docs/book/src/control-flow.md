# Control Flow

`gof` is trying to keep control flow obvious. The current bootstrap subset already
has enough control flow to write meaningful logic without pretending to support
every future feature.

## `if`, `while`, and loop control

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
- `break` exits the nearest loop
- `continue` skips to the next iteration of the nearest loop
- `break` and `continue` are rejected outside loops

That sounds simple, but it matters because the language is rejecting "truthy/falsy"
ambiguity on purpose.

Here is the current loop-control baseline:

```gof
fn main() -> int:
    mut total = 0
    for value in [1, 2, 3, 4]:
        if value == 2:
            continue
        total = total + value
        if total > 3:
            break
    return total
```

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

Payload variants can be destructured directly in the arm header:

```gof
enum JobState:
    Ready
    Running(pid: int)
    Failed(message: string)

fn score(state: JobState) -> int:
    match state:
        JobState.Ready:
            return 0
        JobState.Running(pid):
            return pid
        JobState.Failed(message):
            return len(message)
```

Additional payload rules:

- payload arms must bind the exact number of payload values
- payload bindings become immutable locals inside that arm body

Why this matters:

> `match` is not just prettier branching. It is the current way `gof` makes state
> handling explicit and complete.

That is why exhaustiveness exists already, even while much of the ecosystem is still bootstrap-grade.

`Result` uses the same exhaustive `match` story:

```gof
fn main() -> int:
    outcome: Result[int, string] = Result.Ok(42)
    match outcome:
        Result.Ok(value):
            return value
        Result.Err(error):
            return len(error)
```

## Postfix `?`

`?` is the current short form for recoverable early return.

```gof
fn halve(value: int) -> Result[int, string]:
    if value % 2 != 0:
        return Result.Err("odd")
    return Result.Ok(value / 2)

fn compute() -> Result[int, string]:
    half = halve(84)?
    return Result.Ok(half)
```

Current rules:

- the operand must be `Result[T, E]`
- `Ok(value)` unwraps to `value`
- `Err(error)` returns early from the enclosing function
- the enclosing function must return a compatible `Result[_, E]`

This matters because `gof` is deliberately choosing explicit recoverable control
flow over hidden exception behavior.

## When to use which form

Use:

- `if` when you are deciding based on boolean facts
- `while` when you are stepping through repeated work
- `break` when a loop should stop immediately
- `continue` when one iteration should be skipped cleanly
- `match` when you are branching over a closed enum state
- postfix `?` when you want explicit recoverable early return from a `Result`

That division keeps control flow readable and keeps state transitions honest.
