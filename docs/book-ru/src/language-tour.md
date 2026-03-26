# Обзор языка

Эта глава дает короткую, но честную картину текущего surface языка.

Она не пытается покрыть все edge cases. Ее задача — сначала собрать правильную
модель языка в голове, а уже потом вести тебя глубже.

## Главная идея

Код на `gof` должен читаться просто, но вести себя явно.

Из этого следует, что язык сейчас строится вокруг:

- top-level функций
- явных типовых контрактов
- immutable by default биндингов
- видимой мутабельности через `mut`
- явного моделирования данных через `struct` и `enum`
- явной concurrency-модели через tasks, channels и `select`

## Функции — основная единица исполнения

```gof
fn add(a: int, b: int) -> int:
    return a + b
```

Текущие правила:

- функции живут на top level
- параметры могут быть типизированы
- return type можно объявлять явно
- все return paths должны быть совместимы по типу

## Биндинги immutable по умолчанию

```gof
total = 10
```

Так создается immutable binding.

Если переопределение действительно часть логики, это надо сказать явно:

```gof
mut total: int = 10
total = total + 1
```

Это один из главных правил `gof`: мутация разрешена, но она не должна быть тихой по умолчанию.

## С какими значениями уже можно работать

В текущем bootstrap-subset уже есть:

- `int`
- `string`
- `bool`
- lists
- dicts
- пользовательские `struct`
- пользовательские `enum`
- task values
- channel values
- `unit`

## Builtin helpers

Сейчас есть:

- `len(...)`
- `print(...)`
- `assert(...)`
- `append(...)`
- `contains(...)`
- `read_file(...)`
- `write_file(...)`
- `dict()`
- `insert(...)`
- `channel()`
- `send(...)`
- `recv(...)`

Стандартная библиотека пока маленькая намеренно.

> `gof` лучше иметь маленький, но честный stdlib surface, чем большой набор helper'ов, которые потом будут мешать нормальной архитектуре языка.

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

В этом одном примере уже видно текущую форму языка:

- typed functions
- structs
- methods
- explicit receiver contract
- explicit return value
