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
- structs, enums, and methods
- exhaustive `match`
- lists and dicts
- `assert`
- file I/O
- channels and `select`
- `go` / `await`
- a real CLI and a bootstrap-native build path

That is already enough to teach real semantics and run non-trivial examples.

## Still in progress

The next major steps are:

- richer stdlib growth without semantic mud
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

## Where to look next

- `README.md` for the public project overview
- `roadmap.md` for milestone status
- `spec/` for exact language and diagnostics contracts
- `examples/` for runnable source files
- this book for the teachable explanation layer
