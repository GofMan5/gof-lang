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
- `testing_baseline/math_test.gof`: language-level `test fn`, `TestContext.case(...)`, and deterministic `gof test` discovery
- `testing_baseline/fixture_test.gof`: typed `fixture(module)` / `fixture(test)` injection with cached module scope and per-test temp resources
- `io_roundtrip.gof`: `Result`-based file I/O plus `assert(...)`
- `bytes_stream_roundtrip.gof`: shipped `bytes` / `io` / `time` stdlib foundation with timeout-armed stream wrappers, explicit UTF-8 stream helpers, and roundtrip file I/O
- `line_io.gof`: line-oriented file I/O with `write_lines(...)`, `read_lines(...)`, and explicit cleanup
- `stdin_report.gof`: explicit stdin ingestion through `read_stdin()` and `read_stdin_lines()` for shell-style pipelines
- `time_report.gof`: explicit Unix wall-clock helpers through `unix_seconds()` and `unix_millis()`
- `base64_report.gof`: explicit base64 encode/decode helpers for HTTP/CI/script payloads
- `stdlib_imports.gof`: shipped stdlib import smoke for reserved `bytes`/`io`/`time`/`net`/`http` names plus JSON request and typed response helpers from shipped `http`
- `tcp_roundtrip.gof`: shipped `net` stdlib foundation with loopback listen/connect helpers, typed `SocketAddr` dialing, timeout wrappers, and duplex string roundtrip I/O
- `http_request_report.gof`: structured HTTP request reports with explicit headers, timeout, and response metadata
- `csv_inventory.gof`: explicit CSV parse/stringify helpers on top of file I/O and `Result`
- `config_report.gof`: explicit TOML config parsing through `toml_parse(...)` plus JSON helpers
- `yaml_report.gof`: explicit YAML config parsing through `yaml_parse(...)` plus JSON helpers
- `template_report.gof`: explicit `template_render(...)` over TOML-derived config data for script-friendly text generation
- `process_capture.gof`: explicit `run_process(...)` orchestration with captured status, stdout, stderr, and argv metadata
- `concurrent_squares.gof`: `go`, `await`, simple task-based concurrency
- `task_result_propagation.gof`: `go`, `await`, and task-boundary `Result[..., RuntimeError]` error preservation
- `await_result.gof`: explicit recoverable task joins for plain `task[T]` values
- `await_result_cancellation.gof`: cancellation-aware `await_result(task, token)` joins
- `channel_select.gof`: channels, `Result`-based `send`/`recv`, and `select`
- `channel_capacity.gof`: explicit rendezvous and bounded channel capacities without leaving the typed channel model
- `select_round_robin.gof`: deterministic round-robin `select` arm rotation when multiple receives are ready
- `select_default.gof`: immediate `default` fallback when no `recv(...)` arm is ready
- `select_send.gof`: `select` send-arms plus immediate `default` fallback when a send would block
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
- `telegram_long_polling.gof`: bootstrap Telegram bot path with shipped `http` JSON helpers, explicit timeout wrappers, `env`, `sleep`, and explicit `Result`
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
- `testing_baseline/math_test.gof` -> `gof test examples/testing_baseline/math_test.gof` reports `2 passed; 0 failed`
- `testing_baseline/fixture_test.gof` -> `gof test examples/testing_baseline/fixture_test.gof` reports `2 passed; 0 failed`
- `io_roundtrip.gof` -> `Result.Ok(value: 6)`
- `bytes_stream_roundtrip.gof` -> `Result.Ok(value: 20)`
- `line_io.gof` -> `Result.Ok(value: 19)`
- `stdin_report.gof` -> `Result.Ok(value: chars=11 first=alpha lines=2)` for stdin `alpha\nbeta\n`
- `time_report.gof` -> `Result.Ok(value: 1)`
- `base64_report.gof` -> `Result.Ok(value: 12)`
- `stdlib_imports.gof` -> `Result.Ok(value: 280)`
- `tcp_roundtrip.gof` -> `Result.Ok(value: 10)`
- `http_request_report.gof` -> `Result.Ok(value: 216)` when `GOF_HTTP_REQUEST_BASE` points at a test endpoint that returns `202 Accepted`, body `accepted`, and header `X-Request-Id: req-42`; the example now also demonstrates shipped bearer/custom-header builders over `http_request(...)`
- `csv_inventory.gof` -> `Result.Ok(value: 8)`
- `config_report.gof` -> `Result.Ok(value: 17)`
- `yaml_report.gof` -> `Result.Ok(value: 17)`
- `template_report.gof` -> `Result.Ok(value: service=alpha port=7 workers=5)`
- `process_capture.gof` -> `Result.Ok(value: 7)` when `GOF_PROCESS_EXAMPLE` points to a command that exits successfully for `--help`
- `concurrent_squares.gof` -> `225`
- `task_result_propagation.gof` -> ``RuntimeError.TaskFailed(message: GOF3068: `/` by zero is not allowed)``
- `channel_select.gof` -> `Result.Ok(value: 9)`
- `channel_capacity.gof` -> `Result.Ok(value: 12)`
- `select_round_robin.gof` -> `Result.Ok(value: 16)`
- `select_default.gof` -> `Result.Ok(value: 7)`
- `select_send.gof` -> `Result.Ok(value: 10)`
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

## Process example note

`process_capture.gof` defaults to running `gof --help`. For deterministic local
or CI runs, point `GOF_PROCESS_EXAMPLE` at a known executable that exits
successfully for `--help`.

```text
GOF_PROCESS_EXAMPLE=/path/to/program gof run examples/process_capture.gof
```

## HTTP request example note

`http_request_report.gof` expects a small HTTP endpoint so it can validate the
structured bootstrap `http_request(...)` contract deterministically. Point
`GOF_HTTP_REQUEST_BASE` at a test server that accepts `POST /inspect` and
returns status `202`, body `accepted`, and an `X-Request-Id` header:

```text
GOF_HTTP_REQUEST_BASE=http://127.0.0.1:8080 gof run examples/http_request_report.gof
```

## Package note

`package_app/` is a manifest-backed example. If you edit `package_app/gof.mod` or
move local dependency paths, refresh `package_app/gof.lock` with:

```text
gof mod resolve --dir examples/package_app
```

## Testing note

The testing baseline now supports language-level tests through `test fn` and
the shipped `testing` stdlib:

```text
gof test examples/testing_baseline
gof test --list examples/testing_baseline
gof test examples/testing_baseline/fixture_test.gof
```

Snapshots live under `tests/snapshots/` relative to the nearest project root
and update only when you pass `--update-snapshots`.

Typed fixtures resolve explicitly by parameter name and compatible type:

- `fixture(module) fn name(...)` caches one value per test file
- `fixture(test) fn name(...)` recreates one value per test case
- only `fixture(test)` may request `t: TestContext`
