# gof examples

This directory is the current executable showcase for the bootstrap `gof`
language. Every example is expected to run through:

```text
cargo run -q -p gof-cli --bin gof -- run <example>
```

## Examples

- `calculator.gof`: typed functions, mutable bindings, arithmetic
- `factorial.gof`: `while`, `if`, typed locals, control-flow return contract
- `concurrent_squares.gof`: `go`, `await`, simple task-based concurrency
- `list_sum.gof`: lists, indexing, `len`, looping over data
- `string_metrics.gof`: list of strings, indexing to string, `len(string)`
- `parallel_report.gof`: lists plus task fan-out and aggregation
- `modules/main.gof`: same-directory imports and multi-file resolution
- `portfolio/main.gof`: imports, lists, `while`, `if`, indexing, and typed contracts together

## Expected outputs

- `calculator.gof` -> `107`
- `factorial.gof` -> `120`
- `concurrent_squares.gof` -> `225`
- `list_sum.gof` -> `108`
- `string_metrics.gof` -> `10`
- `parallel_report.gof` -> `104`
- `modules/main.gof` -> `121`
- `portfolio/main.gof` -> `160`
