# Standard Library Baseline

The current standard-library surface is deliberately small.

That is not because the language wants to stay tiny. It is because the project
does not want to teach unstable semantics as if they were settled.

## Output and correctness checks

```gof
print("ready")
assert(true, "must stay true")
```

`print(...)` is the current explicit output primitive.

`assert(...)` is important during bootstrap because it gives examples and tests an
in-language correctness tool without pretending a full testing framework already exists.

Current `assert` rules:

- `assert(condition)` checks a boolean
- `assert(condition, "message")` adds a message
- failed assertions become diagnostics in the bootstrap runtime

## Process, filesystem, and paths

```gof
fn main() -> Result[int, RuntimeError]:
    root = cwd()?
    path = path_join(root, "target/demo.txt")
    write_file(path, "gof")?
    return Result.Ok(len(read_file(path)?))
```

Current operational baseline is intentionally explicit:

- `argv()` returns CLI arguments
- `read_stdin()` reads one cached stdin snapshot as text
- `read_stdin_lines()` exposes the same stdin snapshot through explicit line splitting
- `unix_seconds()` and `unix_millis()` expose the current Unix wall clock through explicit `Result`
- `env(name)` reads one environment variable
- `cwd()` returns the current working directory
- `run_process(program, args)` executes a program directly without shell interpolation and returns captured `program`, `args`, `status`, `stdout`, and `stderr` inside `Result[json, RuntimeError]`
- `read_file(path)` and `write_file(path, contents)` use `Result`
- `read_lines(path)` and `write_lines(path, lines)` keep line-oriented file automation explicit through `Result[list[string], RuntimeError]` and `Result[unit, RuntimeError]`
- `exists(path)`, `read_dir(path)`, `mkdir(path)`, and `remove_file(path)` stay string-based
- path helpers stay explicit through `path_join`, `path_dir`, `path_base`, and `path_ext`

This is not pretending to be a complete I/O library. It is a minimal baseline for
real side effects in CLI tools and bot-style automation.

```gof
fn main() -> Result[int, RuntimeError]:
    report = run_process("gof", ["--help"])?
    args = json_get(report, "args")?
    status = json_int(json_get(report, "status")?)?
    first = json_string(json_index(args, 0)?)?
    return Result.Ok(status + json_len(args)? + len(first))
```

`run_process(...)` is intentionally narrow:

- it does not invoke a shell
- it does not hide quoting or escaping rules
- it keeps argv shape explicit through `list[string]`
- it returns captured output as data instead of printing it implicitly

Shell-style stdin ingestion stays equally explicit:

```gof
fn main() -> Result[string, RuntimeError]:
    text = read_stdin()?
    lines = read_stdin_lines()?
    first_line = first(lines)?
    return template_render("chars={{chars}} first={{first}} lines={{lines}}", {"chars": to_string(len(text)), "first": first_line, "lines": to_string(len(lines))})
```

- `read_stdin()` and `read_stdin_lines()` share one cached stdin snapshot per run
- both helpers keep stdin access inside explicit `Result` flow
- `read_stdin_lines()` follows the same line semantics as `read_lines(path)`

Wall-clock time stays equally explicit:

```gof
fn main() -> Result[int, RuntimeError]:
    seconds = unix_seconds()?
    millis = unix_millis()?
    assert(millis >= seconds * 1000, "expected unix millis to be at least seconds * 1000")
    return Result.Ok(1)
```

- `unix_seconds()` and `unix_millis()` return `Result[int, RuntimeError]`
- host clock failures surface as `RuntimeError.Time(message)`

## Collection helpers

For lists:

```gof
values = [1, 2]
values = append(values, 3)
has_three = contains(values, 3)
window = slice(values, 0, 2)
ordered = sort(reverse(values))
```

For dicts:

```gof
store: dict = {"critical": 5, "ok": 7}
names = keys(store)
counts = values(store)
present = contains(store, "ok")
```

For strings:

```gof
line = trim("  gof,lang  ")
parts = split(line, ",")
merged = join(parts, "-")
has_prefix = starts_with(merged, "gof")
has_suffix = ends_with(merged, "lang")
```

These helpers are teaching an important `gof` habit:

- helper calls should be explicit
- data-shape changes should be visible
- the language should avoid surprising hidden work
- dict views should stay deterministic and allocation-visible
- string helpers should stay unsurprising and allocation-visible

Current string-helper rules:

- `trim(text)` returns a trimmed string
- `split(text, separator)` returns `list[string]`
- `split` rejects an empty separator in the bootstrap contract
- `join(parts, separator)` requires `list[string]`
- `starts_with(text, prefix)` returns `bool`
- `ends_with(text, suffix)` returns `bool`

## Sequence helpers

```gof
fn main() -> Result[int, RuntimeError]:
    values = [7, 1, 5, 3]
    head = first(values)?
    tail = last(values)?
    middle = slice(values, 1, 3)?
    reversed = reverse(values)
    ordered = sort(values)
    smallest = min(values)?
    loudest = max(["warn", "critical", "ok"])?
    return Result.Ok(head + tail + len(middle) + len(reversed) + len(ordered) + smallest + len(loudest))
```

Current sequence-helper rules:

- `first(list)` and `last(list)` return `Result[element, RuntimeError]`
- empty-list `first` and `last` return `RuntimeError.EmptySequence(message)`
- `slice(list, start, end)` returns `Result[list[element], RuntimeError]`
- `slice` rejects negative indices, `start > end`, and `end > len(list)` with `RuntimeError.Slice(message)`
- `reverse(list)` returns a new reversed list
- `sort(list)` returns a new deterministically sorted list
- `sort` currently supports only `list[int]` and `list[string]`
- `min(list)` and `max(list)` return `Result[element, RuntimeError]`
- empty-list `min` and `max` return `RuntimeError.EmptySequence(message)`
- `min` and `max` currently support only `list[int]` and `list[string]`
- sequence helpers are explicit allocation-visible transforms; they do not hide mutation

## JSON, CSV, TOML, YAML, base64, HTTP, and explicit retry delays

```gof
fn notify(base: string) -> Result[int, RuntimeError]:
    sleep(0)
    headers: dict[string] = {"Accept": "application/json", "Content-Type": "application/json"}
    report = http_request("POST", base + "/notify", "{\"text\":\"pong\"}", headers, 1500)?
    return json_int(json_get(report, "status")?)
```

Current JSON, CSV, TOML, YAML, base64, templating, and HTTP rules:

- `json_parse(text)` returns `Result[json, RuntimeError]`
- `json_get(value, key)` and `json_index(value, index)` keep JSON traversal explicit
- `json_len`, `json_string`, and `json_int` perform explicit typed extraction
- `csv_parse(text)` returns `Result[list[list[string]], RuntimeError]`
- `csv_stringify(rows)` returns `Result[string, RuntimeError]`
- malformed CSV text or failed serialization surface as `RuntimeError.Csv(message)`
- `toml_parse(text)` returns `Result[json, RuntimeError]`
- unsupported TOML scalars outside the bootstrap `json` bridge surface as `RuntimeError.Toml(message)`
- `yaml_parse(text)` returns `Result[json, RuntimeError]`
- non-integer YAML numbers, non-string mapping keys, and tagged YAML values surface as `RuntimeError.Yaml(message)`
- `base64_encode(text)` returns an encoded `string`
- `base64_decode(text)` returns `Result[string, RuntimeError]`
- invalid base64 text or decoded bytes outside UTF-8 surface as `RuntimeError.Base64(message)`
- `template_render(template, values)` returns `Result[string, RuntimeError]`
- `template_render(...)` accepts either `dict[...]` values or a top-level `json` object
- `template_render(...)` renders explicit `{{key}}` placeholders and reports malformed placeholders or missing keys as `RuntimeError.Template(message)`
- `http_request(method, url[, body[, headers[, timeout_ms]]])` returns `Result[json, RuntimeError]`
- `http_request(...)` exposes `status`, `body`, `headers`, `method`, and `url` as a structured response report
- `http_request(...)` preserves non-success HTTP statuses as successful reports so callers can branch on them explicitly
- `http_request(...)` normalizes response header names to lowercase and exposes each header as `list[string]`
- `http_request(...)` accepts explicit `dict[string]` request headers and an optional non-negative timeout in milliseconds
- HTTPS/TLS continues to ride on the bootstrap `ureq + rustls` transport path
- `http_get(url)` is the current bootstrap HTTP read path
- `http_post(url, body[, content_type])` is the current bootstrap HTTP write path
- `sleep(milliseconds)` makes retry and backoff intent explicit instead of hiding it in framework magic
- request failures and non-success HTTP statuses become `RuntimeError` values

```gof
fn main() -> Result[string, RuntimeError]:
    config = toml_parse("service = \"alpha\"\nport = 7")?
    return template_render("{{service}} listens on {{port}}", config)
```

```gof
fn main() -> Result[int, RuntimeError]:
    config = yaml_parse("service: alpha\nport: 7\nlimits:\n  workers: 5")?
    limits = json_get(config, "limits")?
    workers = json_int(json_get(limits, "workers")?)?
    name = json_string(json_get(config, "service")?)?
    port = json_int(json_get(config, "port")?)?
    return Result.Ok(len(name) + workers + port)
```

```gof
fn main() -> Result[int, RuntimeError]:
    encoded = base64_encode("gof!")
    decoded = base64_decode(encoded)?
    return Result.Ok(len(encoded) + len(decoded))
```

```gof
fn main() -> Result[int, RuntimeError]:
    headers: dict[string] = {"Accept": "application/json"}
    report = http_request("GET", "https://example.com/health", "", headers, 1500)?
    status = json_int(json_get(report, "status")?)?
    return Result.Ok(status)
```

`template_render(...)` is intentionally narrow:

- it is explicit text expansion, not a full template engine
- it resolves only top-level keys
- it keeps missing-data failures explicit through `RuntimeError.Template(...)`
- it works well with `dict[...]` values and TOML/JSON-driven automation data without inventing a framework layer

That slice is intentionally narrow, but it is already enough for a long-polling
Telegram bot baseline.

## Conversion helpers

```gof
fn main() -> Result[int, RuntimeError]:
    parsed = parse_int(trim(" 41 "))?
    rendered = "gof-" + to_string(parsed + 1)
    return Result.Ok(parsed + len(rendered))
```

Current conversion rules:

- `parse_int(text)` requires a string and returns `Result[int, RuntimeError]`
- invalid numeric text becomes `RuntimeError.ParseInt(message)` instead of hidden fallback behavior
- `to_string(value)` requires one printable value and returns `string`
- conversion stays explicit; `gof` is not trying to teach silent coercions

## Sequence construction

```gof
for value in range(5):
    print(value)

for value in range(2, 12, 4):
    print(value)
```

Current `range` rules:

- `range(stop)` starts at `0` and steps by `1`
- `range(start, stop)` steps by `1`
- `range(start, stop, step)` uses the explicit step
- every `range` argument must be an `int`
- `range` rejects a zero step instead of pretending to know what you meant
- `range` returns an explicit `list[int]`

## Why the stdlib is intentionally narrow

The standard library should grow only when the language contract is stable enough
to support it well.

That means:

- no decorative surface area
- no helper that hides surprising allocations
- no fake convenience that blocks optimizer work later
- no pretending the ecosystem is bigger than it is

The current stdlib is small, but it already teaches the right direction: explicit
helpers, explicit side effects, and explicit data-shape operations.
