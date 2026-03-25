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
- top-level `fn`
- function parameters with optional builtin type annotations
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
- additive and multiplicative expressions
- comparison expressions
- named function calls
- indexing through `list_expr[index_expr]`
- builtin `len(...)` for lists and strings

## Bootstrap binding rules

- `name = expr` creates an immutable local if `name` does not exist yet
- `name: type = expr` creates an immutable local constrained by the declared builtin type
- `mut name = expr` creates a mutable local
- `mut name: type = expr` creates a mutable local constrained by the declared builtin type
- reassigning an immutable local is a compile error
- function parameters can currently be annotated with builtin types `int`, `string`, `bool`, `task`, and `unit`
- function return types can currently be annotated with the same builtin types
- local bindings and return contracts can currently use the builtin `list` annotation
- `import name` currently resolves `name.gof` next to the importing source file and merges top-level functions into one bootstrap module graph
- import cycles are rejected during module graph loading
- duplicate top-level function names across the module graph are rejected
- function calls currently target only top-level named functions
- `len(value)` is currently a builtin recognized by the compiler and evaluator
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
