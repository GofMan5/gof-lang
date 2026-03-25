# Diagnostics Contract

Every compiler diagnostic must provide:

- code
- severity
- primary span
- message
- note
- optional fix-it

## Formatting

Diagnostics must be deterministic. The same source and edition must produce identical codes, spans, and notes across runs.

## Initial codes

- `GOF1001`: invalid indentation
- `GOF1002`: inconsistent dedent
- `GOF2001`: unexpected token
- `GOF2002`: expected token
- `GOF3001`: missing `main` entrypoint for executable mode
- `GOF3002`: unknown local binding
- `GOF3003`: attempted reassignment of immutable binding
- `GOF3004`: unknown function call target
- `GOF3005`: wrong argument count for function call
- `GOF3006`: duplicate local binding in function scope
- `GOF3007`: non-boolean control-flow condition
