# Базовая стандартная библиотека

Текущий stdlib surface намеренно небольшой.

Это не потому, что язык хочет остаться игрушечным. Это потому, что проект не
хочет учить нестабильной семантике как будто она уже устоялась.

## Output и correctness checks

```gof
print("ready")
assert(true, "must stay true")
```

`print(...)` — текущий явный output primitive.

`assert(...)` важен на bootstrap-стадии, потому что дает in-language correctness tool
для examples и tests, не притворяясь полноценным test framework.

Текущие правила `assert`:

- `assert(condition)` проверяет boolean
- `assert(condition, "message")` добавляет сообщение
- failed assertions превращаются в diagnostics в bootstrap runtime

## Process, filesystem и paths

```gof
fn main() -> Result[int, RuntimeError]:
    root = cwd()?
    path = path_join(root, "target/demo.txt")
    write_file(path, "gof")?
    return Result.Ok(len(read_file(path)?))
```

Текущий operational baseline намеренно явный:

- `argv()` возвращает CLI arguments
- `read_stdin()` читает один кэшированный stdin snapshot как текст
- `read_stdin_lines()` раскрывает тот же stdin snapshot через явное разбиение на строки
- `unix_seconds()` и `unix_millis()` дают явный доступ к текущему Unix wall clock через `Result[int, RuntimeError]`
- `env(name)` читает одну переменную окружения
- `cwd()` возвращает current working directory
- `run_process(program, args)` запускает процесс напрямую без shell-интерполяции и возвращает `program`, `args`, `status`, `stdout` и `stderr` внутри `Result[json, RuntimeError]`
- `read_file(path)` и `write_file(path, contents)` используют `Result`
- `read_lines(path)` и `write_lines(path, lines)` дают явный line-oriented I/O через `Result[list[string], RuntimeError]` и `Result[unit, RuntimeError]`
- `exists(path)`, `read_dir(path)`, `mkdir(path)` и `remove_file(path)` остаются string-based
- path helpers остаются явными через `path_join`, `path_dir`, `path_base` и `path_ext`

Это не полноценная I/O-библиотека. Это минимальный baseline для реальных side effects
в CLI tools и bot-style automation.

```gof
fn main() -> Result[int, RuntimeError]:
    report = run_process("gof", ["--help"])?
    args = json_get(report, "args")?
    status = json_int(json_get(report, "status")?)?
    first = json_string(json_index(args, 0)?)?
    return Result.Ok(status + json_len(args)? + len(first))
```

`run_process(...)` специально остается узким:

- он не запускает shell
- он не скрывает quoting и escaping rules
- он держит argv явным через `list[string]`
- он возвращает захваченный вывод как данные, а не печатает его неявно

Ввод из shell pipeline так же остается явным:

```gof
fn main() -> Result[string, RuntimeError]:
    text = read_stdin()?
    lines = read_stdin_lines()?
    first_line = first(lines)?
    return template_render("chars={{chars}} first={{first}} lines={{lines}}", {"chars": to_string(len(text)), "first": first_line, "lines": to_string(len(lines))})
```

- `read_stdin()` и `read_stdin_lines()` делят один кэшированный stdin snapshot на запуск
- обе функции держат stdin внутри явного `Result`-контракта
- `read_stdin_lines()` использует ту же line semantics, что и `read_lines(path)`

Явные wall-clock helpers тоже остаются узкими:

```gof
fn main() -> Result[int, RuntimeError]:
    seconds = unix_seconds()?
    millis = unix_millis()?
    assert(millis >= seconds * 1000, "expected unix millis to be at least seconds * 1000")
    return Result.Ok(1)
```

- `unix_seconds()` и `unix_millis()` возвращают `Result[int, RuntimeError]`
- ошибки host clock поднимаются как `RuntimeError.Time(message)`

## Helpers для коллекций

Для lists:

```gof
values = [1, 2]
values = append(values, 3)
has_three = contains(values, 3)
window = slice(values, 0, 2)
ordered = sort(reverse(values))
```

Для dicts:

```gof
store: dict = {"critical": 5, "ok": 7}
names = keys(store)
counts = values(store)
present = contains(store, "ok")
```

Для строк:

```gof
line = trim("  gof,lang  ")
parts = split(line, ",")
merged = join(parts, "-")
has_prefix = starts_with(merged, "gof")
has_suffix = ends_with(merged, "lang")
```

Эти helpers учат важной привычке `gof`:

- helper calls должны быть явными
- изменение формы данных должно быть видно
- язык должен избегать скрытой неожиданной работы
- dict views должны оставаться детерминированными и не скрывать аллокации
- string helpers тоже не должны скрывать аллокации и странную магию

Текущие правила string helpers:

- `trim(text)` возвращает строку без внешних пробелов
- `split(text, separator)` возвращает `list[string]`
- `split` запрещает пустой разделитель в bootstrap-контракте
- `join(parts, separator)` требует `list[string]`
- `starts_with(text, prefix)` возвращает `bool`
- `ends_with(text, suffix)` возвращает `bool`

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

Текущие правила sequence helpers:

- `first(list)` и `last(list)` возвращают `Result[element, RuntimeError]`
- для пустого списка `first` и `last` возвращают `RuntimeError.EmptySequence(message)`
- `slice(list, start, end)` возвращает `Result[list[element], RuntimeError]`
- `slice` запрещает отрицательные индексы, `start > end` и `end > len(list)` через `RuntimeError.Slice(message)`
- `reverse(list)` возвращает новый список в обратном порядке
- `sort(list)` возвращает новый детерминированно отсортированный список
- `sort` пока поддерживает только `list[int]` и `list[string]`
- `min(list)` и `max(list)` возвращают `Result[element, RuntimeError]`
- для пустого списка `min` и `max` возвращают `RuntimeError.EmptySequence(message)`
- `min` и `max` пока поддерживают только `list[int]` и `list[string]`
- sequence helpers остаются явными allocation-visible преобразованиями и не прячут мутацию

## JSON, CSV, TOML, YAML, HTTP и явные retry delays

```gof
fn notify(base: string) -> Result[string, RuntimeError]:
    sleep(0)
    body = http_post(base + "/notify", "{\"text\":\"pong\"}", "application/json")?
    payload = json_parse(body)?
    status = json_string(json_get(payload, "status")?)?
    return Result.Ok(status)
```

Текущие правила JSON, CSV, TOML, YAML, templating и HTTP:

- `json_parse(text)` возвращает `Result[json, RuntimeError]`
- `json_get(value, key)` и `json_index(value, index)` делают traversal явным
- `json_len`, `json_string` и `json_int` делают явное typed extraction
- `csv_parse(text)` возвращает `Result[list[list[string]], RuntimeError]`
- `csv_stringify(rows)` возвращает `Result[string, RuntimeError]`
- malformed CSV text и failed serialization поднимаются как `RuntimeError.Csv(message)`
- `toml_parse(text)` возвращает `Result[json, RuntimeError]`
- неподдерживаемые TOML scalar values вне bootstrap `json` bridge поднимаются как `RuntimeError.Toml(message)`
- `yaml_parse(text)` возвращает `Result[json, RuntimeError]`
- нецелые YAML numbers, mapping keys не-строки и tagged YAML values поднимаются как `RuntimeError.Yaml(message)`
- `template_render(template, values)` возвращает `Result[string, RuntimeError]`
- `template_render(...)` принимает либо `dict[...]`, либо top-level `json` object
- `template_render(...)` рендерит явные `{{key}}` placeholders и поднимает malformed placeholders или missing keys как `RuntimeError.Template(message)`
- `http_get(url)` — текущий bootstrap HTTP read path
- `http_post(url, body[, content_type])` — текущий bootstrap HTTP write path
- `sleep(milliseconds)` делает retry и backoff намерение явным, а не прячет его в framework magic
- request failures и non-success HTTP statuses становятся значениями `RuntimeError`

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

`template_render(...)` специально остается узким:

- это явный text expansion, а не полноценный template engine
- он разрешает только top-level keys
- missing-data ошибки остаются явными через `RuntimeError.Template(...)`
- он хорошо стыкуется с `dict[...]` и TOML/JSON-driven automation data без выдумывания framework layer

Этот слой намеренно узкий, но его уже хватает для baseline long-polling Telegram bot.

## Conversion helpers

```gof
fn main() -> Result[int, RuntimeError]:
    parsed = parse_int(trim(" 41 "))?
    rendered = "gof-" + to_string(parsed + 1)
    return Result.Ok(parsed + len(rendered))
```

Текущие правила conversion helpers:

- `parse_int(text)` требует строку и возвращает `Result[int, RuntimeError]`
- невалидный numeric text становится `RuntimeError.ParseInt(message)`, а не runtime diagnostic
- `to_string(value)` требует одно printable value и возвращает `string`
- преобразование остается явным; `gof` не учит скрытым coercions

## Построение последовательностей

```gof
for value in range(5):
    print(value)

for value in range(2, 12, 4):
    print(value)
```

Текущие правила `range`:

- `range(stop)` стартует с `0` и идет с шагом `1`
- `range(start, stop)` использует шаг `1`
- `range(start, stop, step)` использует явный шаг
- каждый аргумент `range` должен быть `int`
- `range` запрещает нулевой шаг, а не угадывает, что ты имел в виду
- `range` возвращает явный `list[int]`

## Почему stdlib пока узкая

Стандартная библиотека должна расти только тогда, когда языковой контракт уже
достаточно стабилен.

Это значит:

- никакой декоративной surface area
- никаких helper-ов, скрывающих surprising allocations
- никакого fake convenience, который потом мешает оптимизатору
- никакого притворства, что экосистема больше, чем она есть

Текущий stdlib уже маленький, но он учит правильному направлению: explicit helpers,
explicit side effects и explicit data-shape operations.
