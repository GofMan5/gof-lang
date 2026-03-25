# ADR 0002: Layered Compiler Pipeline

## Status

Accepted

## Decision

Compiler phases are strictly layered:

1. lexer
2. CST
3. AST
4. HIR
5. typed HIR
6. MIR
7. SSA
8. backend IR

## Consequences

- each stage owns its invariants
- lowering is one-way
- shortcuts across layers are treated as architecture violations
