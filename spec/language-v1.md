# gof v1 Language Baseline

## Product contract

`gof` borrows Python's readability and low-ceremony feel, but it is not a Python compatibility layer.

### Non-goals

- CPython bytecode compatibility
- monkey patching
- metaclass-based runtime mutation
- unrestricted reflection
- global interpreter lock
- arbitrary postinstall or build-time script execution in packages

## Core semantics

- ahead-of-time native compilation is the target execution model
- gradual-static typing with local inference
- immutability by default, mutable bindings require `mut`
- explicit recoverable errors via `Result[T, E]`
- `panic` reserved for invariant failures
- structured concurrency for async work

## v1 syntax surface

- `module`
- `fn`
- `struct`
- `enum`
- `protocol`
- `match`
- `async` / `await`
- `select`
- `defer`
- `unsafe`

## Current bootstrap subset

The bootstrap compiler in this repository currently supports:

- local same-directory imports through `import name`
- top-level `struct`
- top-level `enum` with unit variants
- top-level `fn`
- function parameters with optional builtin or known user-defined type annotations
- explicit function return type annotations through `fn name(...) -> type:`
- block indentation with `INDENT` / `DEDENT`
- `return`
- `if` / `else`
- `while`
- `go` for spawning top-level named function calls
- `await` for waiting on task values
- module-level inference of function return types when they can be derived from return expressions
- local bindings through `name = expr`
- typed local bindings through `name: type = expr`
- mutable bindings through `mut name = expr`
- typed mutable bindings through `mut name: type = expr`
- reassignment only for previously mutable bindings
- integer and string literals
- boolean literals through `true` / `false`
- list literals through `[expr, ...]`
- identifiers
- logical operators through `and`, `or`, and `not`
- additive and multiplicative expressions
- comparison expressions
- named function calls
- struct constructor calls through `TypeName(...)`
- enum variant references through `EnumName.Variant`
- statement-level exhaustive `match` over enum values
- struct receiver methods through `fn TypeName.method(...)`
- field access through `value.field`
- method calls through `value.method(...)`
- indexing through `list_expr[index_expr]`
- builtin `len(...)` for lists and strings
- builtin `print(...)` for one printable value
- builtin `append(list, value)` for pure list growth
- builtin `contains(haystack, needle)` for list membership and string substring checks

## Bootstrap binding rules

- `name = expr` creates an immutable local if `name` does not exist yet
- `name: type = expr` creates an immutable local constrained by the declared builtin type or struct type
- `mut name = expr` creates a mutable local
- `mut name: type = expr` creates a mutable local constrained by the declared builtin type or struct type
- reassigning an immutable local is a compile error
- function parameters can currently be annotated with builtin types `int`, `string`, `bool`, `task`, `unit`, and known struct names
- function parameters can currently also be annotated with known enum names
- function return types can currently be annotated with the same builtin types plus known struct and enum names
- local bindings and return contracts can currently use the builtin `list` annotation
- `import name` currently resolves `name.gof` next to the importing source file and merges top-level functions, structs, and enums into one bootstrap module graph
- import cycles are rejected during module graph loading
- duplicate top-level function names across the module graph are rejected
- duplicate struct names across the module graph are rejected
- duplicate enum names across the module graph are rejected
- struct, enum, and function names cannot conflict at top level because constructors, type references, and enum variant access must stay unambiguous
- plain function calls currently target only top-level named functions
- struct constructors currently use positional field order from the declaration
- unit enum variants are values and currently do not carry payloads
- enum equality currently works only between values of the same enum type
- enum declarations currently require unique variant names
- enum variant references currently require a known variant declared on the target enum
- `match value:` currently requires `value` to resolve to a known enum
- each `match` arm currently must use `EnumName.Variant`
- each unit variant can appear at most once inside one `match`
- `match` currently requires an arm for every unit variant declared on the enum
- field access currently requires a struct target and a known field name
- methods currently require an explicit receiver declaration `fn TypeName.method(...)`
- method declarations currently require a known struct receiver type
- the first method parameter currently must resolve to the same struct type as the declared receiver
- method calls currently require a struct receiver value and a known method on that struct
- `and` and `or` currently require boolean operands and preserve short-circuit evaluation
- `not` currently requires a boolean operand
- `len(value)` is currently a builtin recognized by the compiler and evaluator
- `print(value)` is currently a builtin recognized by the compiler and evaluator
- `append(list, value)` is currently a builtin recognized by the compiler and evaluator
- `contains(haystack, needle)` is currently a builtin recognized by the compiler and evaluator
- `print` currently accepts exactly one printable value and returns `unit`
- `append` currently accepts exactly one list plus one compatible value and returns a new list
- `contains` currently accepts either `(list, value)` or `(string, string)` and returns `bool`
- list literals must stay homogeneous once the bootstrap type layer can determine their element types
- indexing currently requires a list target and an integer index
- function return types are inferred across the module until the bootstrap type layer reaches a stable result
- an explicit function return annotation acts as the function contract and must stay compatible with every return path in the body
- every `return` inside one function must resolve to one compatible type
- `go` currently accepts only `go some_function(...)`
- task values carry the inferred return type of the spawned function when known
- `await` currently accepts only task values produced by `go`
- control-flow conditions must evaluate to `bool`

Everything else is specified as future work and intentionally blocked from pretending to be stable.
