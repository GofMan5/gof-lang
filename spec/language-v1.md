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

- top-level `fn`
- function parameters
- block indentation with `INDENT` / `DEDENT`
- `return`
- `if` / `else`
- `while`
- local bindings through `name = expr`
- mutable bindings through `mut name = expr`
- reassignment only for previously mutable bindings
- integer and string literals
- boolean literals through `true` / `false`
- identifiers
- additive and multiplicative expressions
- comparison expressions
- named function calls

## Bootstrap binding rules

- `name = expr` creates an immutable local if `name` does not exist yet
- `mut name = expr` creates a mutable local
- reassigning an immutable local is a compile error
- function calls currently target only top-level named functions
- control-flow conditions must evaluate to `bool`

Everything else is specified as future work and intentionally blocked from pretending to be stable.
