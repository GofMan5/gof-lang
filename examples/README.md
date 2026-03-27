# gof examples

This directory is the current executable showcase for the bootstrap `gof`
language. Every example is expected to run through:

```text
gof run <example>
```

If you are developing `gof` from source without installing it first, the
developer fallback is:

```text
cargo run -q -p gof-cli --bin gof -- run <example>
```

## Examples

- `calculator.gof`: typed functions, mutable bindings, arithmetic
- `factorial.gof`: `while`, `if`, typed locals, control-flow return contract
- `for_report.gof`: `for ... in ...` over lists, strings, and dict keys
- `break_continue.gof`: loop control through `break` and `continue`
- `hello_print.gof`: builtin `print(...)` plus normal return value rendering
- `io_roundtrip.gof`: `Result`-based file I/O plus `assert(...)`
- `concurrent_squares.gof`: `go`, `await`, simple task-based concurrency
- `task_result_propagation.gof`: `go`, `await`, and task-boundary `Result[..., RuntimeError]` error preservation
- `channel_select.gof`: channels, `Result`-based `send`/`recv`, and `select`
- `select_round_robin.gof`: deterministic round-robin `select` arm rotation when multiple receives are ready
- `timeout_cancellation.gof`: timeout-backed cancellation tokens for blocked channel operations
- `channel_lifecycle.gof`: `close(channel)`, `recv(...)`, and `RuntimeError.ChannelClosed`
- `dict_report.gof`: dict literals, dict indexing, and key membership
- `dict_views.gof`: `keys(dict)`, `values(dict)`, deterministic dict views, and list iteration
- `text_helpers.gof`: `trim`, `split`, `join`, `starts_with`, and `ends_with`
- `conversion_helpers.gof`: `Result`-based `parse_int`, `to_string`, and string assembly
- `sequence_helpers.gof`: `first`, `last`, `slice`, `reverse`, deterministic `sort`, and explicit `min`/`max` over lists
- `comparison_surface.gof`: explicit equality domains, lexicographic string ordering, and structural comparisons
- `range_helpers.gof`: `range(stop)`, `range(start, stop)`, and `range(start, stop, step)`
- `numeric_surface.gof`: unary minus, division, modulo, and parameterized builtin types
- `payload_match.gof`: enum payload variants plus exhaustive destructuring `match`
- `result_flow.gof`: `Result[T, E]`, `Result.Ok`, `Result.Err`, postfix `?`, and exhaustive result handling
- `runtime_ops.gof`: `argv`, `env`, `cwd`, `exists`, `read_dir`, and path helpers on top of `Result`
- `telegram_long_polling.gof`: bootstrap Telegram bot path with `env`, `sleep`, `http_get`, `http_post`, `json_*`, and `Result`
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
- `package_app/`: manifest-resolved local package dependency plus package-root import resolution and committed `gof.lock`
- `portfolio/main.gof`: imports, lists, `while`, `if`, indexing, and typed contracts together
- `records/main.gof`: imported `struct`, field access across modules, typed contracts

## Expected outputs

- `calculator.gof` -> `107`
- `factorial.gof` -> `120`
- `for_report.gof` -> `19`
- `break_continue.gof` -> `4`
- `hello_print.gof` -> prints `gof ready`, `42`, then `7`
- `io_roundtrip.gof` -> `Result.Ok(value: 6)`
- `concurrent_squares.gof` -> `225`
- `task_result_propagation.gof` -> ``RuntimeError.TaskFailed(message: GOF3068: `/` by zero is not allowed)``
- `channel_select.gof` -> `Result.Ok(value: 9)`
- `select_round_robin.gof` -> `Result.Ok(value: 16)`
- `timeout_cancellation.gof` -> `42`
- `channel_lifecycle.gof` -> `7`
- `dict_report.gof` -> `11`
- `dict_views.gof` -> `28`
- `text_helpers.gof` -> `10`
- `conversion_helpers.gof` -> `Result.Ok(value: 47)`
- `sequence_helpers.gof` -> `Result.Ok(value: 15)`
- `comparison_surface.gof` -> `Result.Ok(value: 42)`
- `range_helpers.gof` -> `40`
- `numeric_surface.gof` -> `Result.Ok(value: 2)`
- `payload_match.gof` -> `42`
- `result_flow.gof` -> `42`
- `runtime_ops.gof` -> `Result.Ok(value: 6)` with `GOF_RUNTIME_DIR`, `GOF_RUNTIME_MODE`, and two CLI args
- `telegram_long_polling.gof` -> returns the first `update_id` when the API payload contains updates
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
- `package_app/` -> `90`
- `portfolio/main.gof` -> `160`
- `records/main.gof` -> `140`

## Package note

`package_app/` is a manifest-backed example. If you edit `package_app/gof.mod` or
move local dependency paths, refresh `package_app/gof.lock` with:

```text
gof mod resolve --dir examples/package_app
```
