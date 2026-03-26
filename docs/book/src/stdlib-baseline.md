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
- `env(name)` reads one environment variable
- `cwd()` returns the current working directory
- `read_file(path)` and `write_file(path, contents)` use `Result`
- `exists(path)`, `read_dir(path)`, `mkdir(path)`, and `remove_file(path)` stay string-based
- path helpers stay explicit through `path_join`, `path_dir`, `path_base`, and `path_ext`

This is not pretending to be a complete I/O library. It is a minimal baseline for
real side effects in CLI tools and bot-style automation.

## Collection helpers

For lists:

```gof
values = [1, 2]
values = append(values, 3)
has_three = contains(values, 3)
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

## JSON, HTTP, and explicit retry delays

```gof
fn notify(base: string) -> Result[string, RuntimeError]:
    sleep(0)
    body = http_post(base + "/notify", "{\"text\":\"pong\"}", "application/json")?
    payload = json_parse(body)?
    status = json_string(json_get(payload, "status")?)?
    return Result.Ok(status)
```

Current JSON and HTTP rules:

- `json_parse(text)` returns `Result[json, RuntimeError]`
- `json_get(value, key)` and `json_index(value, index)` keep JSON traversal explicit
- `json_len`, `json_string`, and `json_int` perform explicit typed extraction
- `http_get(url)` is the current bootstrap HTTP read path
- `http_post(url, body[, content_type])` is the current bootstrap HTTP write path
- `sleep(milliseconds)` makes retry and backoff intent explicit instead of hiding it in framework magic
- request failures and non-success HTTP statuses become `RuntimeError` values

That slice is intentionally narrow, but it is already enough for a long-polling
Telegram bot baseline.

## Conversion helpers

```gof
parsed = parse_int(trim(" 41 "))
rendered = "gof-" + to_string(parsed + 1)
```

Current conversion rules:

- `parse_int(text)` requires a string and returns `int`
- invalid numeric text becomes a runtime diagnostic instead of hidden fallback behavior
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
