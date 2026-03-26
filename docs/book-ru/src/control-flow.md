# Управление потоком

`gof` пытается держать control flow очевидным. Даже текущий bootstrap-subset уже
дает достаточно, чтобы писать нормальную логику без фальшивой “богатой” surface area.

## `if` и `while`

```gof
fn main() -> int:
    mut n: int = 3
    mut total: int = 0
    while n > 0:
        if n > 1:
            total = total + n
        else:
            total = total + 10
        n = n - 1
    return total
```

Текущие правила:

- conditions должны быть boolean
- блоки задаются отступами
- comparison operators и logical operators питают эти conditions

## `and`, `or`, `not`

Булева логика типизирована явно:

```gof
if ready and not failed:
    return 1
```

Текущие правила:

- `and` и `or` требуют boolean operands
- `not` требует boolean operand
- `and` и `or` сохраняют short-circuit behavior

## `match`

`match` сейчас statement-level и исчерпывающий по enum values.

```gof
enum Status:
    Ready
    Busy
    Failed

fn score(status: Status) -> int:
    match status:
        Status.Ready:
            return 100
        Status.Busy:
            return 50
        Status.Failed:
            return 0
```

Текущие правила:

- target должен быть enum
- все варианты должны быть покрыты
- duplicate arms запрещены
- arms от другого enum запрещены

Главная мысль:

> `match` нужен не просто для красоты, а чтобы делать state handling явным и полным.

## Когда использовать что

Используй:

- `if` — когда ветвишься по булевым фактам
- `while` — когда повторяешь работу
- `match` — когда ветвишься по закрытому enum-state
