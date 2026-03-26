# Базовая стандартная библиотека

Текущий stdlib surface намеренно маленький.

Это не потому, что язык хочет остаться игрушечным. Это потому, что проект не
хочет учить нестабильной семантике как будто она уже устоялась.

## Output и correctness checks

```gof
print("ready")
assert(true, "must stay true")
```

`print(...)` — текущий явный output primitive.

`assert(...)` важен на bootstrap-стадии, потому что дает in-language correctness tool для examples и tests, не притворяясь полноценным test framework.

Текущие правила `assert`:

- `assert(condition)` проверяет boolean
- `assert(condition, "message")` добавляет сообщение
- failed assertions превращаются в diagnostics в bootstrap runtime

## File I/O

```gof
fn main() -> int:
    path = "target/demo.txt"
    write_file(path, "gof")
    return len(read_file(path))
```

Текущий file I/O intentionally direct:

- `read_file(path)` возвращает строку
- `write_file(path, contents)` записывает строку

## Helpers для коллекций

Для lists:

```gof
values = [1, 2]
values = append(values, 3)
has_three = contains(values, 3)
```

Для dicts:

```gof
mut store: dict = dict()
store = insert(store, "name", 1)
present = contains(store, "name")
```

Эти helpers учат важной привычке `gof`:

- helper calls должны быть явными
- изменение формы данных должно быть видно
- язык должен избегать скрытой неожиданной работы

## Почему stdlib пока узкая

Стандартная библиотека должна расти только тогда, когда языковой контракт уже достаточно стабилен.

Это значит:

- никакой декоративной surface area
- никаких helper'ов, скрывающих surprising allocations
- никакого fake convenience, который потом мешает оптимизатору
- никакого притворства, что экосистема больше, чем она есть
