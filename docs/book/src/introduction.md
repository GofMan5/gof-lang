# Introduction

`gof` is a new language project that aims for:

- Python-like readability
- Go-like concurrency and operational simplicity
- Rust-grade engineering discipline
- a clean path to native performance

This is not a Python compatibility layer. The syntax stays readable, but the semantics are being designed for predictability, optimization, and correctness.

## What this book is for

This book is the main learning path for `gof`.

Use it when you want:

- the beginner-friendly explanation of what the language does
- runnable examples that match the current implementation
- one place that evolves with the compiler, runtime, and CLI

## What this book is not

This book is not the formal language contract. For exact rules, use:

- `spec/language-v1.md`
- `spec/diagnostics.md`
- `rfcs/`

The book teaches. The spec defines.

## Current reality

`gof` is still in bootstrap development. That matters.

Today the repo already has:

- functions
- modules through same-directory imports
- `struct`
- `enum`
- exhaustive `match`
- methods
- lists
- dicts
- `assert`
- file I/O
- channels and `select`
- `go` / `await`
- `gof build --native`

But some pieces are still bootstrap-grade:

- the native build path currently packages the bootstrap evaluator
- the standard library is still small
- package management is not finished
- concurrency semantics still need production hardening
