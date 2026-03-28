# AGENTS.md

This file is the working contract for humans and agents in `gof`.

It exists to keep changes fast, correct, and non-chaotic.
If a rule here conflicts with convenience, convenience loses.

## 1. Project target

`gof` is not a toy syntax experiment.

Target profile:

- Python-like readability and DX
- Go-like operational simplicity and concurrency
- Rust-grade reliability, explicitness, and engineering discipline
- C and C++ level cost-model awareness and systems reach where they matter
- C# and Java grade tooling clarity, API discipline, and large-codebase ergonomics
- JavaScript grade scripting practicality and fast feedback loops where that helps
- predictable runtime and toolchain behavior

The bar is not "good enough". The bar is "credible as a serious language".

`gof` is a language synthesis project, not a clone of one ancestor.

It should combine the strongest properties of:

- Python
- Go
- Rust
- C
- C++
- C#
- Java
- JavaScript

Rule:

- borrow strengths, not historical baggage
- do not add a feature just because another language has it
- prefer coherent semantics over familiarity cosplay
- reject dynamic chaos, hidden magic, undefined behavior as a norm, accidental complexity, and platform bloat

## 2. Priority order

When tradeoffs are real, use this order:

1. correctness
2. reliability and safety
3. runtime performance
4. compile-time performance
5. DX
6. surface-area growth

Do not sacrifice semantics or architecture just to imitate another language.

## 3. Non-negotiables

Never:

- merge half-finished public behavior
- hide instability behind README polish
- add hacky shortcuts across compiler layers
- keep silent TODOs that mask architectural debt
- solve a local task by making the long-term design worse

Always:

- finish the slice end-to-end
- add tests for behavior changes
- make contracts explicit
- prefer extending good structure over inventing parallel structure

## 4. Architecture discipline

Compiler pipeline stays layered:

1. lexer
2. CST
3. AST
4. HIR
5. typed HIR
6. MIR
7. SSA
8. backend IR / artifact

Forbidden:

- parser-level typing hacks
- backend knowledge inside frontend parsing
- undocumented IR invariants
- hidden public behavior switches
- mixed frontend/runtime logic without a clear boundary

Every layer must have:

- a clear responsibility
- explicit invariants
- one-way dependencies
- direct tests

## 5. Language guardrails

`gof` must stay explicit and predictable.

Baseline rules:

- immutability by default
- `mut` only when explicit
- recoverable errors through typed results
- `panic` only for broken invariants
- no implicit shared mutable state as the default model
- unsafe/FFI only behind explicit safe boundaries

Do not add a feature unless it is clear:

- who owns the data
- where values live
- where allocation happens
- where blocking can happen
- how races are prevented
- how the feature is debugged

## 6. Concurrency rules

Concurrency features are product features, not syntax decoration.

Required for serious concurrency work:

- semantics
- diagnostics contract
- integration tests
- stress plan
- benchmark plan
- cancellation story
- panic/error propagation story

Do not hide race-prone behavior behind "nice" syntax.

## 7. Testing and verification

Every behavior change needs tests.

Minimum expected coverage:

- new feature: positive + negative tests
- bug fix: regression test
- new diagnostic: fixture or unit test
- performance-sensitive change: benchmark or baseline

Use minimal sufficient verification, not ritual full-suite spam.

Expected verification by area:

- parser / AST / HIR / typechecker / runtime: focused Rust tests + conformance where relevant
- CLI: CLI/conformance tests
- `tools/vscode-gof`: `npm test` and `npm run package`
- docs/book: build touched books
- release/install scripts: targeted packaging checks

Run the whole world only when the change is truly cross-cutting or release-risky.

When reporting verification, say what was run and why it is sufficient.

## 8. Docs, spec, roadmap

Public behavior is not done until the repository teaches it.

If you change public syntax, semantics, tooling, stdlib, diagnostics, runtime
behavior, or package behavior, update the relevant source of truth:

- `docs/book/`
- `examples/`
- `README.md`
- `spec/`
- `roadmap.md`

Use:

- RFC for public behavior changes
- ADR for architecture decisions

If code changed but roadmap/spec/docs did not, the feature is not complete.

## 9. VS Code extension policy

`tools/vscode-gof` is a first-class product surface.

Any syntax, diagnostics, snippet, formatting, or editing-surface change must
evaluate whether the extension also needs an update.

If extension-visible behavior changes, update:

- grammar/snippets/docs as needed
- tests
- packaging validation

Required verification for extension-visible changes:

- `npm test`
- `npm run package`

Distribution rule:

- the packaged extension artifact stays in `tools/vscode-gof/`
- do not attach the VS Code package to snapshot releases by default
- manual Marketplace publication is the primary release path

## 10. Release policy

For repository pushes intended to go outward:

- verify locally first
- use `scripts/push-and-release.ps1` or the equivalent standardized path
- do not use ad hoc push + release command soup

For Windows releases, prefer normal platform-grade installers over custom hacks.

## 11. Agent behavior

Agents must:

- finish the requested slice, not stop at analysis
- state real blockers clearly
- avoid unrelated churn in a mixed worktree
- stage only task-relevant files when the worktree is mixed
- choose the correct branch/push target explicitly when the user asks

Agents must not:

- fake completion
- silently skip necessary tests
- hide unfinished work
- bloat the task with unnecessary bureaucracy or redundant full-suite runs

If the architecture is weak, improve it. Do not normalize garbage.

## 12. Definition of done

A task is done only when:

- the code is complete
- architecture boundaries remain clean
- relevant tests pass
- formatting/packaging checks pass when applicable
- docs/spec/roadmap are updated when the public contract changed
- diagnostics are updated when error behavior changed
- no temporary public garbage was left behind

For narrow changes, targeted verification is enough if it really covers the
affected contract.
