# gof Governance

## Stability tiers

- `experimental`: only for isolated internal work, not documented as public surface
- `preview`: user-visible, edition-gated, with removal or stabilization milestone
- `stable`: covered by spec, conformance tests, and diagnostics commitments

## Editions

- Language evolution is edition-based.
- `gof.mod` pins the edition.
- Deprecations must survive at least one full edition before removal.

## Ownership

- Compiler phase owners protect CST/AST/HIR/MIR/SSA boundaries.
- Runtime owners approve allocator, GC, scheduler, and FFI changes.
- Tooling owners approve CLI, formatter, doc tooling, and package manager behavior.

## Diagnostics policy

Every diagnostic must define:

- unique code
- primary span
- human-readable message
- note or context
- actionable fix-it when applicable

## Quality and performance policy

- The project targets 100 percent coverage for deterministic compiler and runtime code.
- New files should not land without direct tests or fixture coverage.
- Benchmark regressions require explicit investigation before merge.
- Runtime speed, compile-time discipline, and stability outrank convenience shortcuts.
