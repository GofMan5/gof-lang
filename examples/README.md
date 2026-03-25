# gof examples

This directory is the current executable showcase for the bootstrap `gof`
language. Every example is expected to run through:

```text
cargo run -q -p gof-cli --bin gof -- run <example>
```

## Examples

- `calculator.gof`: typed functions, mutable bindings, arithmetic
- `factorial.gof`: `while`, `if`, typed locals, control-flow return contract
- `hello_print.gof`: builtin `print(...)` plus normal return value rendering
- `io_roundtrip.gof`: bootstrap file I/O plus `assert(...)`
- `concurrent_squares.gof`: `go`, `await`, simple task-based concurrency
- `channel_select.gof`: channels, `send`, `recv`, and `select`
- `dict_report.gof`: `dict`, `insert`, dict indexing, and key membership
- `geometry.gof`: `struct`, typed fields, constructor calls, field access
- `geometry_methods.gof`: receiver methods on structs and method calls
- `health_gate.gof`: `and`, `or`, `not`, short-circuit-friendly control flow
- `list_sum.gof`: lists, indexing, `len`, looping over data
- `stdlib_helpers.gof`: `append`, `contains`, `len`, and control flow together
- `status_report.gof`: `enum`, unit variants, typed enum bindings, equality
- `status_match.gof`: exhaustive `match` over enum values
- `string_metrics.gof`: list of strings, indexing to string, `len(string)`
- `parallel_report.gof`: lists plus task fan-out and aggregation
- `modules/main.gof`: same-directory imports and multi-file resolution
- `portfolio/main.gof`: imports, lists, `while`, `if`, indexing, and typed contracts together
- `records/main.gof`: imported `struct`, field access across modules, typed contracts

## Expected outputs

- `calculator.gof` -> `107`
- `factorial.gof` -> `120`
- `hello_print.gof` -> prints `gof ready`, `42`, then `7`
- `io_roundtrip.gof` -> `6`
- `concurrent_squares.gof` -> `225`
- `channel_select.gof` -> `9`
- `dict_report.gof` -> `11`
- `geometry.gof` -> `42`
- `geometry_methods.gof` -> `10`
- `health_gate.gof` -> `1`
- `list_sum.gof` -> `108`
- `stdlib_helpers.gof` -> `18`
- `status_report.gof` -> `200`
- `status_match.gof` -> `50`
- `string_metrics.gof` -> `10`
- `parallel_report.gof` -> `104`
- `modules/main.gof` -> `121`
- `portfolio/main.gof` -> `160`
- `records/main.gof` -> `140`
