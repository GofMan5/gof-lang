# Типы и данные

Эта глава объясняет, как данные моделируются в текущем bootstrap-subset `gof`,
и так же честно фиксирует, чего в языке пока еще нет.

## Встроенные scalar values

Сейчас базовые scalar types такие:

- `int`
- `string`
- `bool`
- `json`

Есть также `unit`, который означает "нет осмысленного значения", и runtime-level
values вроде tasks, channels и cancellation tokens, которые появляются через
отдельные языковые конструкции.

Когда нужен явный типовой контракт на встроенные контейнеры и async-values,
текущий bootstrap surface поддерживает parameterized annotations:

- `list[int]`
- `dict[int]`
- `channel[int]`
- `task[int]`
- `Result[int, RuntimeError]`

## Списки

Списки — первый встроенный последовательный контейнер.

```gof
values: list[int] = [10, 20, 30]
return values[0] + len(values)
```

Текущие правила:

- список должен быть однородным, когда type layer может это вывести
- индексация требует `int`
- negative indexing пока не поддерживается
- выход за границы ловится в bootstrap evaluator
- `append(list, value)` возвращает новый список
- `first(list)` и `last(list)` возвращают `Result[element, RuntimeError]`
- `slice(list, start, end)` возвращает `Result[list[element], RuntimeError]`
- `reverse(list)` возвращает новый список в обратном порядке
- `sort(list)` возвращает новый детерминированно отсортированный список для `list[int]` и `list[string]`
- `min(list)` и `max(list)` возвращают `Result[element, RuntimeError]` для `list[int]` и `list[string]`

Главная идея здесь такая:

> List helpers в `gof` — это явное построение нового значения, а не скрытая магическая мутация.

## Словари

Текущий dict baseline уже имеет честный literal-syntax для обычного случая:

```gof
store: dict[int] = {"ok": 7, "warn": 2}
return store["ok"] + len(store)
```

Если словарь нужно строить пошагово, `insert(...)` остается явным инструментом:

```gof
base: dict[int] = {"ok": 7}
next = insert(base, "warn", 2)
return next["warn"]
```

Текущие правила dict:

- ключи сейчас должны резолвиться в `string`
- значения в literal сейчас должны быть совместимы по типу
- `dict()` создает пустой словарь
- `insert(dict, key, value)` возвращает новый dict
- `keys(dict)` возвращает `list[string]` в детерминированном порядке ключей
- `values(dict)` возвращает значения в том же детерминированном порядке
- индексация требует существующий ключ
- `contains(dict, "key")` проверяет наличие ключа
- `len(dict)` возвращает число записей

Чего пока специально нет:

- нестроковые ключи
- пользовательский generic surface syntax
- скрытая мутация за literal-syntax или helper-ами

## Числовые выражения

Текущий bootstrap numeric surface уже поддерживает:

```gof
value = -(8 + 2) / 5
rest = value % 2
```

Текущие правила:

- unary `-` требует `int`
- `/` и `%` пока работают только на `int`
- деление и modulo на ноль дают runtime diagnostic

## Сравнения

Сравнения теперь разделены на два явных контракта:

- equality через `==` и `!=`
- ordering через `<`, `<=`, `>`, `>=`

Текущие правила:

- equality работает для `int`, `string`, `bool`, `json`, `unit`, `list[T]`, `dict[T]`, однотипных `struct`, однотипных `enum` и совместимых `Result[T, E]`, если все вложенные значения тоже допускают equality
- ordering работает только для `int` и `string`
- строки сравниваются лексикографически
- `channel`, `task` и `cancel_token` не участвуют в сравнениях даже внутри более крупных значений

## Struct

`struct` — текущий пользовательский aggregate type.

```gof
struct Point:
    x: int
    y: int

fn total(point: Point) -> int:
    return point.x + point.y
```

Текущие правила:

- поля объявляются в типе
- конструктор использует порядок полей из объявления
- field access требует реальный `struct`
- неизвестные поля запрещены

## Enum

`enum` — способ моделировать конечные состояния. Сейчас он поддерживает и
unit-варианты, и payload-варианты.

```gof
enum JobState:
    Ready
    Running(pid: int)
    Failed(message: string)
```

Текущие правила:

- имена вариантов должны быть уникальны
- payload-поля объявляются прямо в варианте и имеют явные типы
- unit-вариант используется как `EnumName.Variant`
- payload-вариант создается как `EnumName.Variant(value, ...)`
- однотипные enum участвуют в structural equality, если их payload-поля допускают equality

## Result

`Result[T, E]` — текущий baseline для recoverable errors.

```gof
fn halve(value: int) -> Result[int, string]:
    if value % 2 != 0:
        return Result.Err("odd")
    return Result.Ok(value / 2)
```

Текущие правила:

- `Result[T, E]` — встроенный parameterized type
- `Result.Ok(value)` создает success payload
- `Result.Err(error)` создает error payload
- result-значения обрабатываются через исчерпывающий `match`
- result-значения участвуют в equality, если обе payload-стороны допускают equality
- postfix `expr?` распаковывает `Ok(value)` и делает ранний `return` на `Err(error)`
- внешняя функция должна возвращать совместимый `Result[_, E]`

Operational helpers уже используют тот же контракт:

- `env("NAME")` возвращает `Result[string, RuntimeError]`
- `parse_int(text)` возвращает `Result[int, RuntimeError]`
- `first(values)` возвращает `Result[T, RuntimeError]`
- `slice(values, start, end)` возвращает `Result[list[T], RuntimeError]`
- `min(values)` возвращает `Result[T, RuntimeError]`
- `max(values)` возвращает `Result[T, RuntimeError]`
- `read_file(path)` возвращает `Result[string, RuntimeError]`
- `recv(channel)` возвращает `Result[T, RuntimeError]`
- JSON, CSV, TOML и HTTP helpers тоже возвращают `Result`

Это важно, потому что error propagation остается единым для CLI tooling, file I/O,
concurrency и бот-ориентированного network code.

## RuntimeError

`RuntimeError` — встроенный operational error enum.

Сейчас он включает:

- `EnvMissing(name: string)`
- `Io(message: string)`
- `ChannelClosed`
- `Cancelled`
- `TaskFailed(message: string)`
- `TaskPanicked(task: string)`
- `ParseInt(message: string)`
- `EmptySequence(message: string)`
- `Slice(message: string)`
- `Json(message: string)`
- `Csv(message: string)`
- `Toml(message: string)`
- `HttpRequest(message: string)`
- `HttpStatus(code: int, body: string)`

Именно он делает `?` полезным сразу в нескольких operational областях, не вводя
скрытых conversion rules.

## Методы

Методы в `gof` явные, а не магические.

```gof
fn Point.with_bonus(self: Point, extra: int) -> int:
    return self.x + self.y + extra
```

Главное правило:

- receiver type объявляется в имени метода
- первый параметр должен совпадать с этим receiver type

## Как правильно думать о данных в `gof`

Полезное правило:

- scalar — для простых значений
- list — для упорядоченных однородных данных
- dict — для string-keyed lookup
- struct — для shaped data
- enum — для закрытого набора состояний
- Result — для явного recoverable success/error flow

Такой набор различий делает control flow, concurrency и later stdlib гораздо
чище и предсказуемее.
