<div align="center">
  <h1>gof</h1>
  <p><strong>Python-like readability. Go-like concurrency. Rust-grade engineering discipline.</strong></p>
  <p>Build automation, services, and systems-oriented tooling without runtime chaos, semantic mud, or fake ergonomics.</p>

  <p>
    <a href="https://gofman5.github.io/gof-lang/"><img alt="Docs" src="https://img.shields.io/badge/docs-gof_book-0f172a?style=for-the-badge&logo=gitbook&logoColor=white"></a>
    <a href="https://gofman5.github.io/gof-lang/ru/"><img alt="Docs RU" src="https://img.shields.io/badge/docs-ru_book-1d4ed8?style=for-the-badge&logo=gitbook&logoColor=white"></a>
    <a href="spec/language-v1.md"><img alt="Spec" src="https://img.shields.io/badge/spec-language_v1-111827?style=for-the-badge&logo=readthedocs&logoColor=white"></a>
    <a href="https://marketplace.visualstudio.com/items?itemName=gofman5.gof-language"><img alt="VS Code Extension" src="https://img.shields.io/badge/VS_Code-extension-0098ff?style=for-the-badge&logo=visualstudiocode&logoColor=white"></a>
    <a href="LICENSE"><img alt="License" src="https://img.shields.io/badge/license-MIT-22c55e?style=for-the-badge"></a>
  </p>
</div>

```gof doctest
enum JobState:
    Ready
    Running(pid: int)
    Failed(message: string)

fn score(state: JobState) -> int:
    match state:
        JobState.Ready:
            return 0
        JobState.Running(pid):
            return pid
        JobState.Failed(message):
            return len(message)

fn main() -> int:
    return score(JobState.Running(42))
```

> **Bootstrap stage.** The compiler, evaluator, and toolchain are functional and actively developed.
> Native builds work via `gof build --native`, which currently packages the bootstrap evaluator plus a deterministic embedded source bundle that preserves reachable imports and local package context. Direct codegen backend is still in progress.

## Install

**Unix/macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.sh | bash
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.ps1 | iex
```

Run the same command again to update. The installer places the binary in `~/.gof/bin` and updates PATH.
After installation, open a new shell and use `gof` directly. Rust is not required to run `gof` programs.

## Usage

```bash
gof run examples/geometry.gof          # run a program
gof run --watch examples/geometry.gof  # rerun on edits
gof check examples/geometry.gof        # compile-only validation
gof test examples/testing_baseline     # run language-level tests
gof test --list examples/testing_baseline
gof test --docs                        # run opt-in markdown doctests
gof mod resolve --dir examples/package_app
gof build app.gof --native             # build a native executable
gof fmt src/main.gof                   # format
gof test tests/fixtures                # run diagnostics/runtime fixtures
```

## What works today

| Area | Status |
|------|--------|
| Functions, typed params, parameterized type annotations, return contracts | stable |
| `struct`, payload `enum`, exhaustive `match` with destructuring | stable |
| `Result[T, E]`, `Result.Ok`, `Result.Err`, postfix `?`, exhaustive `match` over `Result` | stable |
| Methods, field access, constructors | stable |
| `if`/`else`, `while`, `for`, `break`, `continue`, logical ops, explicit equality/ordering, unary `-`, `/`, `%` | stable |
| Lists, dict literals, indexing, dict views | stable |
| `print`, `assert`, `argv`, `read_stdin()`, `read_stdin_lines()`, `env`, `cwd`, `run_process(...)`, file and line I/O, path/fs helpers, string helpers, sequence helpers (`first`/`last`/`slice`/`reverse`/`sort`/`min`/`max`), conversion helpers, `base64_encode(...)`, `base64_decode(...)`, `range`, `sleep(...)`, `unix_seconds()`, `unix_millis()` | stable |
| JSON, CSV, TOML, YAML, and `template_render(...)` helpers plus bootstrap `http_get(...)` / `http_post(...)` / `http_request(...)` | bootstrap |
| Same-directory imports, reserved shipped stdlib imports (`bytes` / `io` / `time` / `net` / `http` / `testing`), initial `Bytes` / stream / deadline / TCP stdlib foundation, and manifest-resolved local path packages with deterministic `gof.lock` | bootstrap |
| `go`, `await`, `await_result(task[, token])`, typed channels, `close`, capacity-aware channels, cancellation tokens, `select` | bootstrap |
| `test fn`, `fixture(scope) fn`, shipped `testing` stdlib, snapshot-aware and JSON-reporting `gof test`, opt-in markdown doctests, and hybrid discovery across `*_test.gof`, `tests/**/*.gof`, and legacy repo fixtures | bootstrap |
| `gof build --native` | bootstrap (wraps evaluator) |
| `gof check --json [--stdin]` | stable compiler-backed diagnostics contract |
| `gof run --watch [--debounce-ms] <target> [-- ...args]` | bootstrap serial edit-run loop for scripts and executable packages |
| Formatter, conformance tests | stable |

See the [language spec](spec/language-v1.md) for the full contract.

Manifest-backed packages now require a committed, fresh `gof.lock` for `gof run`,
`gof build`, and package-aware `gof test`. Refresh it explicitly with
`gof mod resolve --dir <package-root>`.

Shipped stdlib imports now resolve through reserved module names:

- `import bytes`
- `import io`
- `import time`
- `import net`
- `import http`
- `import testing`

Those names no longer shadow to same-directory files or local dependency aliases.
If user code tries to reuse one of those names, the compiler reports an explicit
reserved-stdlib conflict instead of silently picking the wrong module graph.

The shipped `testing` module now provides a real typed language-level test
surface:

- `test fn name(...)` at top level
- `fixture(test) fn name(...)` and `fixture(module) fn name(...)`
- typed `TestContext` helpers such as `t.equal(...)`, `t.match_snapshot(...)`,
  `t.case(...)`, `t.temp_dir()`, `t.temp_file(...)`, `t.env(...)`, `t.skip(...)`,
  and `t.todo(...)`
- deterministic snapshot storage under `tests/snapshots/`
- discovery through `*_test.gof` and `tests/**/*.gof`
- opt-in markdown doctests through fenced `gof doctest ...` blocks plus
  `gof test --docs`

The current shipped testing contract is still explicit and intentionally narrow:

- `test fn` may take zero or more typed fixture parameters plus at most one
  `t: TestContext`
- fixture dependencies resolve by parameter name and compatible type
- fixtures must declare a scope plus an explicit return type of either `Type`
  or `Result[Type, RuntimeError]`
- `fixture(module)` values are cached once per language test file, while
  `fixture(test)` values are recreated once per test case
- fixture values may declare an optional receiver method
  `cleanup() -> unit | Result[unit, RuntimeError]`; test-scoped cleanup runs
  before `TestContext` teardown and module-scoped cleanup runs once per test file
- tests currently return either `unit` or `Result[unit, RuntimeError]`
- `gof test` currently ships list/filter/fail-fast/nocapture/snapshot-update/
  docs/json flows, language-level tests, legacy fixtures, and artifact-backed
  product fixtures under `tests/ui`, `tests/runtime`, and `tests/runtime-fail`
- `--update-snapshots` currently rewrites both snapshots and product-fixture
  artifacts such as `.diag`, `.stdout`, `.stderr`, and `.exit`
- `gof test --json` emits a single machine-readable report with stable schema
  `gof.test.report/v1`, summary counts, per-target events, captured stdout, and
  structured diagnostics
- JUnit reporting, property/fuzz/stress, and benchmark integration stay
  on the roadmap rather than being implied as done

Useful commands:

```text
gof test examples/testing_baseline
gof test --list examples/testing_baseline
gof test --json examples/testing_baseline
gof test --docs
gof test --update-snapshots path/to/project
```

The first shipped stdlib networking foundation now lives behind those reserved
imports instead of behind ever-growing global builtins:

- `bytes`: `Bytes`, `bytes_from_string`, `bytes_to_string`, `bytes_len`, `bytes_slice`, `bytes_concat`
- `io`: `ReadStream`, `WriteStream`, `open_read_stream`, `open_write_stream`
- `time`: `NetDeadline`, `deadline_after`, `deadline_at_unix_millis`
- `net`: `DuplexStream`, `TcpListener`, `SocketAddr`, `connect_tcp`, `connect_tcp_with_control`, `listen_tcp`

This is still bootstrap surface, not the final network platform. The current
focus is explicit `Result`-based bytes/stream/deadline/TCP contracts that keep
allocation and blocking points visible while the typed HTTP client/server
layers are still being built.

`gof run --watch` now gives a script-first fast edit-run loop for single-file
scripts and executable packages. It keeps exactly one run in flight, batches
rapid edits with a small debounce window, and reuses the same execution path as
normal `gof run` so compile/runtime failures stay visible instead of being
hidden behind watch-specific shortcuts.

For manifest-backed packages, watch mode always monitors the root package and
refreshes dependency watch roots from the current fresh `gof.lock` graph. If
the package graph becomes stale, the loop reports the package error and waits
for the next change instead of exiting.

Package-aware `gof test` is intentionally split today:

- executable package targets (`src/main.gof`) perform compile plus execute smoke
- library package targets (`src/lib.gof`) currently perform compile-only validation

`select` now rotates its polling start arm in a deterministic round-robin
baseline when multiple send/receive arms are already ready, but scheduler-level
fairness is still a roadmap item rather than a finished guarantee.

`select` also has a bootstrap `default:` arm baseline plus direct
`send(channel, value)` arms, so non-blocking fallback loops and bounded-channel
backpressure paths do not need to fake readiness through helper channels.

`select` currently prepares each send/receive operation once at select-entry
before polling, so arm expressions with side effects are not re-evaluated on
every poll pass.

The automation stdlib now also has an explicit process orchestration baseline
through `run_process(program, args)`, which executes a program directly without
shell interpolation and returns captured `status`, `stdout`, `stderr`, and
argv metadata inside a `Result[json, RuntimeError]` report.

Shell-oriented scripts can now read process input explicitly through
`read_stdin()` and `read_stdin_lines()`, which cache one stdin snapshot per run
and keep shell pipeline data flow inside normal `Result` contracts.

Automation scripts can now also read wall-clock time explicitly through
`unix_seconds()` and `unix_millis()`, which return `Result[int, RuntimeError]`
instead of hiding host clock failures behind implicit globals.

The script/data-wrangling slice now also has an explicit templating baseline
through `template_render(template, values)`, which expands `{{key}}`
placeholders from either `dict[...]` values or a top-level `json` object
without introducing hidden framework state into the bootstrap runtime.

Config/data ingestion now also includes `yaml_parse(text)`, which keeps YAML
automation input on the same explicit `Result[json, RuntimeError]` path as
`toml_parse(text)` without inventing a separate schema layer.

Automation and HTTP-style scripting now also have explicit base64 text helpers
through `base64_encode(text)` and `base64_decode(text)`, where decoding stays on
the normal `Result[string, RuntimeError]` path and rejects invalid base64 text
or non-UTF-8 decoded bytes as `RuntimeError.Base64(message)`.

HTTP automation now also has a structured bootstrap request path through
`http_request(method, url[, body[, headers[, timeout_ms]]])`. It returns
`Result[json, RuntimeError]` with explicit `status`, `body`, `headers`,
`method`, and `url` fields so bots, webhooks, and internal API scripts can
manage custom headers, non-2xx responses, and timeout policy without falling
back to host-language glue. HTTPS/TLS continues to ride on the existing
`ureq + rustls` transport path instead of inventing a parallel custom TLS
stack.

Cancellation now also has a timeout-backed baseline through
`timeout_token(milliseconds)` and `cancel_after(token, milliseconds)`, while
full deadline/context propagation remains future work.

Task joins now also have an explicit recoverable baseline through
`await_result(task)` and `await_result(task, token)`, which return
`Result[T, RuntimeError]` for any task without changing the existing
plain-`await` semantics. The optional token lets join-sites time out or cancel
without changing the spawned function contract.

Channels now also have an explicit capacity baseline:

- `channel()` keeps the bootstrap unbounded queue path for existing examples
- `channel(0)` creates a rendezvous channel that blocks `send(...)` until a receiver takes the value
- `channel(n)` for `n > 0` creates a bounded channel that applies backpressure when the buffer is full

## Documentation

| | |
|---|---|
| **[The gof Book](https://gofman5.github.io/gof-lang/)** | Learning path - start here |
| **[The gof Book (RU)](https://gofman5.github.io/gof-lang/ru/)** | Russian edition |
| **[VS Code Extension](https://marketplace.visualstudio.com/items?itemName=gofman5.gof-language)** | Install `gof Programming Language` from the Marketplace |
| [Examples](examples/) | Runnable programs |
| [Detailed plans](plans/roadmap/00-index.md) | Ordered implementation plans by area |
| [Telegram bot example](examples/telegram_long_polling.gof) | Long-polling baseline |
| [VS Code extension source](tools/vscode-gof/) | Editor package source, diagnostics plumbing, snippets, and grammar |
| [Language spec](spec/language-v1.md) | Formal contract |
| [Diagnostics spec](spec/diagnostics.md) | Error behavior |
| [Roadmap](roadmap.md) | Milestones and current focus |
| [RFCs](rfcs/) | Language evolution proposals |
| [ADRs](adrs/) | Architecture decisions |

## Building and hacking from source

You only need Rust if you are developing the compiler, runtime, or CLI itself.
Normal language usage should go through the installed `gof` binary.

Requires Rust toolchain (see [rust-toolchain.toml](rust-toolchain.toml)) if you
want to work on the repository internals.

```bash
cargo test --workspace                 # run all tests
cargo run -p gof-cli -- run app.gof    # developer fallback without installing gof
```

Preview the books locally:

```bash
mdbook build docs/book
mdbook build docs/book-ru
```

Build the VS Code extension locally:

```bash
cd tools/vscode-gof
npm install
npm test
npm run package
```

The extension uses the real compiler through `gof check --json --stdin`, so
syntax and type diagnostics stay aligned with the language contract instead of
drifting into editor-only heuristics. `npm run package` leaves the packaged
extension artifact directly in `tools/vscode-gof/` for manual Marketplace
upload.

## Project structure

```text
compiler/    compiler frontend, typing, IR, evaluator
runtime/     runtime contracts
stdlib/      standard library (in progress)
tools/       CLI toolchain (gof-cli)
tools/vscode-gof/ VS Code extension, diagnostics integration, and packaging
tests/       conformance fixtures
benchmarks/  benchmark harness
spec/        language and diagnostics contracts
rfcs/        language proposals
adrs/        architecture decisions
docs/book/   English book
docs/book-ru/ Russian book
examples/    runnable .gof programs
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [GOVERNANCE.md](GOVERNANCE.md).

## License

[MIT](LICENSE)
