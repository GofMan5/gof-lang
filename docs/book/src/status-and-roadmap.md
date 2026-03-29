# Status and Roadmap

The best way to understand `gof` is to separate three things:

- what the language aims to become
- what is already implemented
- what is still bootstrap or intentionally incomplete

If you mix those three layers together, you will either underestimate the project
or believe promises that have not been earned yet.

## Already real

The current repo already supports:

- typed functions
- structs, payload enums, and methods
- exhaustive `match`
- lists, dicts, and parameterized builtin types
- `Result[T, E]` and postfix `?`
- `assert`
- process, filesystem, and path helpers
- JSON helpers and bootstrap HTTP GET
- channels, `close`, cancellation tokens, and `select`
- `go` / `await`
- language-level `test fn`, typed `fixture(scope) fn`, shipped `testing` stdlib, snapshot-aware `gof test`, and opt-in markdown doctests
- a real CLI and a bootstrap-native build path
- an installable VS Code editor baseline with syntax highlighting and snippets for `.gof` files

That is already enough to teach real semantics and run non-trivial examples.
It is also enough to express a first long-polling Telegram bot baseline.

## Still in progress

The next major steps are:

- richer stdlib growth without semantic mud
- deeper testing slices: unified product harnesses, reporters, property/fuzz/stress, and benchmark gates
- production-grade concurrency contracts
- package system hardening
- direct native code generation

These are not cosmetic milestones. They are the layers required for `gof` to move
from "strong bootstrap language" toward "serious production language."

## How to use the roadmap correctly

Use `roadmap.md` to answer:

- what milestone the project is actually working on
- which checkpoints are finished
- which pieces are intentionally deferred

Do not use the roadmap as marketing. Use it as an engineering truth source.

Use `plans/roadmap/` when you need the ordered implementation program behind one
area instead of the status summary. The testing platform is the first major
program decomposed that way.

## Where to look next

- `README.md` for the public project overview
- `roadmap.md` for milestone status
- `plans/roadmap/` for ordered multi-slice implementation plans
- `spec/` for exact language and diagnostics contracts
- `examples/` for runnable source files
- this book for the teachable explanation layer
