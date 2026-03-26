# RFC 0014: Payload Enum Baseline

## Summary

Extend `gof` enums from unit-only variants to payload variants with typed named
fields, plus payload destructuring in `match`.

## Motivation

Unit-only enums are enough for simple state labels, but they are not enough for
real error values, state machines, task results, or the upcoming `Result[T, E]`
baseline. `gof` needs payload enums before it can add a real error model
without inventing a one-off special case.

## Surface

Declaration:

```gof
enum JobState:
    Ready
    Running(pid: int)
    Failed(message: string)
```

Construction:

```gof
current: JobState = JobState.Running(42)
```

Destructuring:

```gof
match current:
    JobState.Ready:
        return 0
    JobState.Running(pid):
        return pid
    JobState.Failed(message):
        return len(message)
```

## Rules

- Variant names remain unique within the enum.
- Payload field names are local to the variant declaration.
- Unit variants use `EnumName.Variant`.
- Payload variants use `EnumName.Variant(value, ...)`.
- Constructor arity must match the declared payload field count.
- Payload values must match declared payload field types.
- `match` remains exhaustive.
- Each variant can appear at most once in one `match`.
- Payload `match` arms must bind exactly one name per payload field.
- Payload bindings become immutable locals inside that arm body.

## Diagnostics

- `GOF3028`: unknown enum variant
- `GOF3030`: duplicate `match` arm for one variant
- `GOF3031`: wrong enum used in a `match` arm
- `GOF3033`: non-exhaustive `match`
- `GOF3069`: wrong payload constructor arity
- `GOF3071`: wrong payload binding arity in a `match` arm

## Non-goals

- No wildcard arms.
- No nested pattern matching.
- No payload destructuring outside `match`.
- No user-defined generic enums in this milestone.
