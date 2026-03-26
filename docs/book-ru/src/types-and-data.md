# Типы и данные

Эта глава объясняет, как данные моделируются в текущем bootstrap-subset и, что
не менее важно, чего там пока еще нет.

## Базовые scalar values

Сейчас базовые scalar types такие:

- `int`
- `string`
- `bool`

Есть также `unit`, а еще runtime-level values вроде tasks и channels, которые появляются через специальные языковые конструкции.

## Списки

Списки — первый встроенный последовательный контейнер.

```gof
values = [10, 20, 30]
return values[0] + len(values)
```

Текущие правила:

- список должен быть однородным, когда type layer может это вывести
- индексация требует `int`
- negative indexing не поддерживается
- выход за границы ловится в bootstrap evaluator
- `append(list, value)` возвращает новый список

Главная идея:

> `append(...)` — это явное построение нового значения, а не скрытая магическая мутация.

## Словари

Текущий dict baseline выглядит так:

```gof
mut store: dict = dict()
store = insert(store, "ok", 7)
store = insert(store, "warn", 2)
return store["ok"] + len(store)
```

Текущие правила dict:

- ключи — строки
- `dict()` создает пустой словарь
- `insert(dict, key, value)` возвращает новый dict
- индексация требует существующий ключ
- `contains(dict, "key")` проверяет наличие ключа
- `len(dict)` возвращает число записей

Чего пока нет:

- dict literals
- нестроковые ключи
- полноценный generic surface syntax

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

`enum` — способ моделировать конечные состояния.

```gof
enum Status:
    Ready
    Busy
    Failed
```

Текущие правила:

- варианты сейчас unit-only
- имена вариантов должны быть уникальны
- equality работает только внутри одного enum type

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
