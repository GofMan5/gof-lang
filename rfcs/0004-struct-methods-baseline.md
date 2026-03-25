# RFC 0004: Struct Methods Baseline

## Summary

Add explicit receiver methods for structs through declarations like:

```gof
fn Point.total(self: Point, extra: int) -> int:
    return self.x + self.y + extra
```

and calls like:

```gof
point.total(5)
```

## Motivation

`gof` already supports structs, fields, enums, and `match`, but real domain logic still
looks too procedural without receiver-style behavior. Methods let data and behavior live
closer together without hidden runtime mutation or namespace hacks.

## Scope

- struct receivers only
- explicit receiver declaration in the function head
- explicit first receiver parameter in the parameter list
- static dispatch only
- method calls lowered to regular function-like calls with an explicit receiver argument

## Static Rules

- method declarations use `fn TypeName.method(...)`
- the receiver type must be a known struct
- the first parameter must resolve to that same struct type
- duplicate methods for the same struct name and method name are rejected across the module graph
- method calls require a struct receiver value

## Diagnostics

- `GOF3034`: duplicate method in the module graph
- `GOF3035`: invalid method receiver contract
- `GOF3036`: unknown method on a struct
- `GOF3037`: method call on a non-struct target

## Non-Goals

- enum methods
- implicit `self`
- trait/protocol dispatch
- method values or bound method objects
