# Обзор языка

Эта глава дает короткую, но честную картину текущего surface `gof`.

Она не пытается описать все edge cases. Ее задача — быстро собрать правильную
ментальную модель языка перед более детальным чтением.

## Базовая идея

Код на `gof` должен читаться просто, но вести себя явно.

Сейчас язык строится вокруг:

- top-level функций
- явных type contracts
- immutable bindings по умолчанию
- видимой мутации через `mut`
- явного моделирования данных через `struct` и `enum`
- явного управления циклом через `break` и `continue`
- явной recoverable error model через `Result`
- явной concurrency через tasks, channels и `select`

## Функции — основной unit выполнения

```gof
fn add(a: int, b: int) -> int:
    return a + b
```

Текущие правила:

- функции живут на top level
- параметры могут иметь type annotations
- return type можно задать явно
- все return paths должны быть типово совместимы

На текущей bootstrap-стадии обычные вызовы пока адресуют top-level named functions.

## Bindings immutable, пока ты явно не выбрал другое

```gof
total = 10
```

Это создает immutable binding.

Если логике нужна reassignment, это нужно сказать явно:

```gof
mut total: int = 10
total = total + 1
```

Mutation в `gof` разрешена, но она не является silent default.

## С какими значениями уже можно работать

Текущий bootstrap subset уже поддерживает:

- `int`
- `string`
- `bool`
- `json`
- списки
- словари
- пользовательские `struct`
- пользовательские `enum`
- `Result[T, E]`
- task values
- channel values
- cancellation-token values
- `unit`

Этого уже хватает для нетривиальных примеров, но еще недостаточно, чтобы
притворяться полной production-экосистемой.

## Встроенные helper-ы пока нарочно компактные

Сейчас surface языка включает:

- `len(...)`
- `print(...)`
- `assert(...)`
- `append(...)`
- `contains(...)`
- `trim(...)`
- `split(...)`
- `join(...)`
- `starts_with(...)`
- `ends_with(...)`
- `parse_int(...)`
- `to_string(...)`
- `range(...)`
- `sleep(...)`
- `argv()`
- `read_stdin()`
- `read_stdin_lines()`
- `unix_seconds()`
- `unix_millis()`
- `yaml_parse(...)`
- `base64_encode(...)`
- `base64_decode(...)`
- `env(...)`
- `cwd()`
- `run_process(...)`
- `read_file(...)`
- `write_file(...)`
- `exists(...)`
- `read_dir(...)`
- `mkdir(...)`
- `remove_file(...)`
- `path_join(...)`
- `path_dir(...)`
- `path_base(...)`
- `path_ext(...)`
- `dict()`
- `insert(...)`
- `keys(...)`
- `values(...)`
- `channel()`
- `close(...)`
- `send(...)`
- `recv(...)`
- `cancel_token()`
- `cancel(...)`
- `is_cancelled(...)`
- `json_parse(...)`
- `json_stringify(...)`
- `json_get(...)`
- `json_index(...)`
- `json_len(...)`
- `json_string(...)`
- `json_int(...)`
- `template_render(...)`
- `http_request(...)`
- `http_get(...)`
- `http_post(...)`

Небольшой размер stdlib — осознанный выбор.

> `gof` лучше иметь маленький, но честный набор helper-ов, чем большой surface,
> который учит неправильной семантике.

## Маленькая программа, которая уже ощущается как `gof`

```gof
struct Point:
    x: int
    y: int

fn Point.total(self: Point, extra: int) -> int:
    return self.x + self.y + extra

fn main() -> int:
    point: Point = Point(3, 4)
    return point.total(5)
```

Один этот пример уже показывает текущую форму языка:

- typed functions
- structs
- methods
- явный receiver contract
- явный return value

И тот же стиль уже распространяется на operational code:

- side effects остаются явными helper-вызовами
- recoverable failures идут через `Result`
- concurrency остается message-passing-first
- бот-ориентированные программы уже можно выражать без притворства, что у языка есть полноценный web framework
