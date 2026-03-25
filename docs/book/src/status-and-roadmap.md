# Status and Roadmap

The best way to understand `gof` is to separate three things:

- what the language aims to become
- what is already implemented
- what is still bootstrap or intentionally incomplete

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

## Still in progress

The next major steps are:

- richer stdlib growth without semantic mud
- production-grade concurrency contracts
- package system hardening
- direct native code generation

## Where to look next

- `README.md` for the public project overview
- `roadmap.md` for milestone status
- `spec/` for exact language and diagnostics contracts
- `examples/` for runnable source files
