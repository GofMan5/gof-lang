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
- `GOF3004`: unknown or invalid callable target
- `GOF3005`: wrong argument count for function call
- `GOF3006`: duplicate local binding in function scope
- `GOF3007`: non-boolean control-flow condition
- `GOF3008`: invalid `go` spawn target
- `GOF3009`: invalid `await` operand
- `GOF3010`: spawned task panicked before producing a value
- `GOF3011`: incompatible return types inside one function
- `GOF3012`: unknown builtin type annotation
- `GOF3013`: annotated binding, parameter, or declared function return type mismatch
- `GOF3014`: unresolved local import
- `GOF3015`: import cycle in the local module graph
- `GOF3016`: duplicate top-level function across the local module graph
- `GOF3017`: incompatible element types inside one list literal
- `GOF3018`: invalid list indexing operation
- `GOF3019`: invalid operand for builtin `len`
- `GOF3020`: duplicate top-level struct across the local module graph
- `GOF3021`: conflicting top-level name between a struct, enum, and/or function
- `GOF3022`: unknown field on a struct value
- `GOF3023`: duplicate field inside one struct declaration
- `GOF3024`: field access on a non-struct value
- `GOF3025`: non-boolean operand for `and` or `or`
- `GOF3026`: non-boolean operand for `not`
- `GOF3027`: duplicate variant inside one enum declaration
- `GOF3028`: unknown enum variant reference
- `GOF3029`: duplicate top-level enum across the local module graph
- `GOF3030`: duplicate `match` arm for one enum variant
- `GOF3031`: invalid `match` arm pattern or wrong enum arm
- `GOF3032`: non-enum `match` target
- `GOF3033`: non-exhaustive `match`
- `GOF3034`: duplicate method across the local module graph
- `GOF3035`: invalid method receiver contract
- `GOF3036`: unknown method on a struct
- `GOF3037`: method call on a non-struct value
- `GOF3038`: invalid operand for builtin `print`
- `GOF3039`: invalid operand for builtin `append`
- `GOF3040`: invalid operand for builtin `contains`
- `GOF3041`: invalid operand for builtin `assert`
- `GOF3042`: assertion failed at runtime
- `GOF3043`: invalid operand for bootstrap file I/O builtins
- `GOF3044`: bootstrap file I/O operation failed
- `GOF3045`: invalid operand for bootstrap dict insert
- `GOF3046`: invalid channel operation
- `GOF3047`: invalid `select` contract or arm
- `GOF3048`: invalid iterable operand for `for`
