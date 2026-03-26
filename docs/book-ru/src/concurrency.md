# Многопоточность

Concurrency — одна из причин существования `gof`, поэтому и учить ее нужно аккуратно.

Текущая реализация уже дает реальный baseline, но это все еще bootstrap baseline.
То есть concurrent examples писать уже можно, но это еще не финальная production contract.

## Шаг 1: `go` и `await`

```gof
fn square(x: int) -> int:
    return x * x

fn main() -> int:
    job = go square(12)
    return await job
```

Текущие правила:

- `go` спавнит top-level named function call
- результат — task value
- `await` ждет этот task value

## Шаг 2: каналы

```gof
fn main() -> int:
    ch: channel = channel()
    send(ch, 4)
    return recv(ch) + 1
```

Текущий channel baseline:

- `channel()` создает bootstrap channel
- `send(channel, value)` отправляет одно значение
- `recv(channel)` получает одно значение

Важная честная оговорка:

> В source language пока нет явного `channel[T]` syntax. Runtime и type layer уже знают про channel values, но пользовательская surface-модель payload types пока еще узкая.

## Шаг 3: `select`

```gof
fn main() -> int:
    left: channel = channel()
    right: channel = channel()
    send(right, 8)
    select:
        value = recv(left):
            return 0
        value = recv(right):
            return value + 1
```

Текущий контракт `select`:

- поддерживаются только receive arms
- arm должен быть `recv(channel):` или `value = recv(channel):`
- bootstrap runtime опрашивает arms, пока один receive не сработает

## Чего еще не хватает

Это уже реальный baseline, но не финальная concurrency story.

Еще не хватает:

- cancellation
- close semantics
- fairness guarantees
- richer propagation rules
- production scheduler hardening

## Как правильно читать текущую concurrency-модель

Думай о ней так:

- уже достаточно реальна, чтобы учиться
- уже достаточно реальна, чтобы тестироваться
- еще недостаточно зрелая, чтобы обещать production-grade semantics
