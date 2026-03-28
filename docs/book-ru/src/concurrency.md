# Многопоточность

Concurrency — одна из причин существования `gof`, поэтому и учить ее нужно аккуратно.

Текущая реализация уже дает реальный baseline, но это все еще bootstrap baseline.
То есть concurrent examples писать уже можно, но это еще не финальная production contract.

## Шаг 1: `go` и `await`

```gof
fn square(x: int) -> int:
    return x * x

fn main() -> int:
    job: task[int] = go square(12)
    return await job
```

Текущие правила:

- `go` спавнит top-level named function call
- результат — task value
- `await` ждет этот task value
- `await_result(task)` делает recoverable join для любого task как `Result[T, RuntimeError]`, не меняя обычный контракт `await`
- `await_result(task, token)` позволяет join-site вернуть `Result.Err(RuntimeError.Cancelled)`, если token отменен до завершения task
- если spawned-функция явно объявлена как `-> Result[..., RuntimeError]`,
  task-boundary evaluator failures теперь возвращаются как
  `Result.Err(RuntimeError.TaskFailed(...))` или
  `Result.Err(RuntimeError.TaskPanicked(...))`, а не ломают caller сразу

Если для plain `task[T]` нужен recoverable join уже сейчас, используй `await_result(task)`:

```gof
fn lucky() -> int:
    return 7

fn main() -> Result[int, RuntimeError]:
    job: task[int] = go lucky()
    return await_result(job)
```

```gof
fn slow() -> int:
    sleep(25)
    return 7

fn main() -> Result[int, RuntimeError]:
    job: task[int] = go slow()
    return await_result(job, timeout_token(0))
```

Ментальная модель такая:

- `go` создает concurrent work
- `await` возвращает это вычисление обратно в текущий flow

## Шаг 2: каналы и явный lifecycle

```gof
fn main() -> Result[int, RuntimeError]:
    ch: channel[int] = channel()
    send(ch, 4)?
    return recv(ch)
```

Текущий channel baseline:

- `channel[T]` уже существует как parameterized builtin type annotation
- `channel()` создает bootstrap channel
- `channel(0)` создает rendezvous channel baseline
- `channel(n)` при `n > 0` создает bounded channel baseline
- `close(channel)` закрывает канал явно
- `send(channel, value)` возвращает `Result[unit, RuntimeError]`
- `recv(channel)` возвращает `Result[T, RuntimeError]`

Сейчас runtime уже различает:

- успешную отправку/получение
- закрытый канал
- отмененное ожидание

Текущие правила capacity:

- `channel()` сохраняет существующий unbounded queue-backed bootstrap path
- `channel(0)` блокирует `send(...)`, пока receiver не заберет значение
- `channel(n)` при `n > 0` блокирует `send(...)`, когда буфер заполнен, пока `recv(...)` не освободит место

## Шаг 3: `select`

```gof
fn main() -> Result[int, RuntimeError]:
    left: channel[int] = channel()
    right: channel[int] = channel()
    send(right, 8)?
    select:
        value = recv(left):
            return Result.Ok(value? + 0)
        value = recv(right):
            return Result.Ok(value? + 1)
```

Текущий контракт `select`:

- поддерживаются receive arms, send arms и один `default` arm
- receive arm должен быть `recv(channel):`, `value = recv(channel):`, `recv(channel, token):` или `value = recv(channel, token):`
- send arm должен быть `send(channel, value):`, `value = send(channel, value):`, `send(channel, value, token):` или `value = send(channel, value, token):`
- `default:` выполняется сразу, если в текущем проходе опроса ни один send/receive arm не готов
- bootstrap runtime подготавливает каждую send/receive операцию один раз на входе в `select`, а затем опрашивает уже подготовленные операции, пока одна из них не вернет `Result.Ok(...)` или `Result.Err(...)`
- если несколько send/receive arms уже готовы, bootstrap runtime вращает стартовый arm по детерминированному round-robin baseline, чтобы первый arm в исходнике не выигрывал всегда

Этого уже хватает для честного message-passing выбора без красивого синтаксиса,
за которым ничего нет.

## Шаг 4: cancellation

```gof
fn main() -> bool:
    token = timeout_token(0)
    manual = cancel_token()
    cancel_after(manual, 0)
    return is_cancelled(token) and is_cancelled(manual)
```

Текущий baseline cancellation:

- `cancel_token()` создает cooperative token
- `cancel(token)` переводит token в cancelled state
- `is_cancelled(token)` читает текущее состояние
- `timeout_token(milliseconds)` создает token, который сам отменяется после неотрицательной задержки
- `cancel_after(token, milliseconds)` планирует отмену уже существующего token через неотрицательную задержку
- `send(..., token)` и `recv(..., token)` наблюдают token во время блокировки

## Чего еще не хватает

Это уже реальный baseline, но не финальная concurrency story.

Еще не хватает:

- production-grade fairness гарантий сверх текущего round-robin polling baseline
- автоматического сохранения task panic и boundary failures для plain `await task` joins без явного `await_result(task)`
- deadline/context propagation сверх timeout-backed token baseline
- production scheduler hardening

## Как правильно читать текущую concurrency-модель

Думай о ней так:

- уже достаточно реальна, чтобы учиться
- уже достаточно реальна, чтобы тестироваться
- еще недостаточно зрелая, чтобы обещать production-grade semantics
