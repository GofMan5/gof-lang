# Управление потоком

`gof` старается держать control flow очевидным. Текущий bootstrap-subset уже
дает достаточно, чтобы писать осмысленную логику, не притворяясь "полноценным"
production surface там, где язык еще не дошел до этой стадии.

## `if`, `while` и loop control

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
- `if` и `while` используют блоки по отступам
- comparison operators и logical operators питают эти conditions
- `break` выходит из ближайшего цикла
- `continue` переходит к следующей итерации ближайшего цикла
- `break` и `continue` запрещены вне циклов

Это кажется простым, но принцип здесь важный: язык специально не вводит
truthy/falsy-неоднозначность.

Текущий baseline для loop control:

```gof
fn main() -> int:
    mut total = 0
    for value in [1, 2, 3, 4]:
        if value == 2:
            continue
        total = total + value
        if total > 3:
            break
    return total
```

## `and`, `or`, `not`

Булева логика в `gof` явно типизирована:

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

- target должен быть enum или `Result`
- все варианты должны быть покрыты
- duplicate arms запрещены
- arms от другого enum запрещены

Payload-варианты можно распаковывать прямо в заголовке arm:

```gof
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
```

Дополнительные правила:

- payload-arm должен объявлять ровно столько binding-ов, сколько payload-значений у варианта
- эти binding-и становятся неизменяемыми локалами внутри тела arm

`Result` использует тот же честный exhaustive story:

```gof
fn main() -> int:
    outcome: Result[int, string] = Result.Ok(42)
    match outcome:
        Result.Ok(value):
            return value
        Result.Err(error):
            return len(error)
```

Главная мысль:

> `match` нужен не просто для красоты, а чтобы делать state handling явным и полным.

## Postfix `?`

`?` - текущая короткая форма для recoverable early return.

```gof
fn halve(value: int) -> Result[int, string]:
    if value % 2 != 0:
        return Result.Err("odd")
    return Result.Ok(value / 2)

fn compute() -> Result[int, string]:
    half = halve(84)?
    return Result.Ok(half)
```

Текущие правила:

- operand должен быть `Result[T, E]`
- `Ok(value)` распаковывается в `value`
- `Err(error)` делает ранний `return` из внешней функции
- внешняя функция должна возвращать совместимый `Result[_, E]`

Это важно, потому что `gof` сознательно выбирает явный recoverable control flow,
а не скрытую exception-магию.

## Когда использовать что

Используй:

- `if` - когда ветвишься по булевым фактам
- `while` - когда повторяешь работу до условия
- `break` - когда цикл нужно остановить сразу
- `continue` - когда одну итерацию нужно чисто пропустить
- `match` - когда ветвишься по закрытому state space
- postfix `?` - когда нужен явный ранний выход из `Result`

Такое разделение держит control flow читаемым и делает переходы состояний честными.
