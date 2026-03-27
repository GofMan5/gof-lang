# Introduction

<section class="docs-home-hero">
  <div class="docs-kicker">The gof Book</div>
  <h1 class="docs-home-title">Learn the language by the right mental model, not by guesswork.</h1>
  <p class="docs-home-deck">
    <code>gof</code> is aiming for Python-like readability, Go-like concurrency, and
    Rust-grade engineering discipline. The important part is not the slogan. The
    important part is that the language is being designed to stay optimizable,
    explicit, and teachable from day one.
  </p>
  <div class="docs-pill-row">
    <div class="docs-pill"><strong>Readable</strong><br>low ceremony, indentation-aware syntax</div>
    <div class="docs-pill"><strong>Explicit</strong><br>typed contracts, visible mutability, clear control flow</div>
    <div class="docs-pill"><strong>Concurrent</strong><br><code>go</code>, <code>await</code>, channels, and <code>select</code></div>
    <div class="docs-pill"><strong>Honest</strong><br>bootstrap features are called bootstrap features</div>
  </div>
  <div class="docs-card-grid">
    <a class="docs-card" href="./getting-started.html">
      <span class="docs-card__eyebrow">Start here</span>
      <span class="docs-card__title">Getting Started</span>
      <span class="docs-card__body">Install or run locally, execute your first program, and learn the command flow.</span>
    </a>
    <a class="docs-card" href="./language-tour.html">
      <span class="docs-card__eyebrow">Mental model</span>
      <span class="docs-card__title">Language Tour</span>
      <span class="docs-card__body">Understand what the language is trying to be before you memorize syntax.</span>
    </a>
    <a class="docs-card" href="./concurrency.html">
      <span class="docs-card__eyebrow">Why gof exists</span>
      <span class="docs-card__title">Concurrency</span>
      <span class="docs-card__body">See the current task, channel, and <code>select</code> baseline with its real limits.</span>
    </a>
  </div>
  <div class="docs-inline-note">
    Canonical rule: the book teaches, the spec defines, and the examples prove
    what really runs today.
  </div>
</section>

## What `gof` is trying to be

`gof` is not trying to be a clone of Python internals. It is trying to keep the
good parts of Python's reading experience while keeping a clean path to native
performance, predictable concurrency, and long-term maintainability.

That leads to a very important rule:

> If "looks a bit like Python" conflicts with safety, optimizer-friendliness, or
> architectural clarity, `gof` chooses clarity.

You should read the current language with this mindset:

- readable syntax matters
- explicit semantics matter more
- bootstrap convenience is allowed
- bootstrap lies are not allowed

## What this book is for

This book is the main learning path for `gof`.

Use it when you want:

- a guided explanation of the language surface that exists today
- examples that match the current compiler and evaluator
- the correct mental model for what is stable, what is bootstrap, and what is still evolving

## What this book is not

This book is not the formal language contract. For exact rules and diagnostics,
use:

- `spec/language-v1.md`
- `spec/diagnostics.md`
- `rfcs/`

The book teaches. The spec defines. The examples keep both honest.

## The right way to learn `gof`

Do not learn `gof` as a pile of keywords.

Learn it in this order:

1. what values and bindings mean
2. what type contracts are already real
3. how control flow stays explicit
4. how user-defined data is modeled
5. how tasks, channels, and `select` behave today
6. what the bootstrap toolchain really emits

That order matters because `gof` is being built around semantics first, syntax second.

## Current reality

`gof` is still in bootstrap development. That matters, and the book will keep saying
that clearly.

Today the repository already has:

- functions and typed return contracts
- same-directory imports plus manifest-resolved local path packages with deterministic lockfiles
- `struct`
- `enum`
- exhaustive `match`
- methods
- lists and dicts
- `assert`
- file I/O
- `go` / `await`
- channels and `select`
- `gof build --native`

But some pieces are still bootstrap-grade:

- the native build path currently packages the bootstrap evaluator
- the standard library is intentionally small
- package management is still local-package-only with no registry or solver
- concurrency semantics still need production hardening

## What you should not assume

Do not assume that `gof` already has:

- Python compatibility
- a full package ecosystem
- production-grade channel semantics
- direct native code generation
- a large standard library

The book will always prefer a precise "not yet" over a misleading "basically yes."
