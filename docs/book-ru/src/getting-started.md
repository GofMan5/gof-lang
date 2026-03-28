# Быстрый старт

Эта глава нужна, чтобы максимально коротким, но правильным путем дойти от нуля
до реально работающей программы на `gof`.

## Выбери нормальную стартовую точку

Есть два хороших пути.

### Путь A: использовать релизный бинарь

Если тебе нужен установленный toolchain:

- Unix-like системы: `curl -fsSL https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.sh | bash`
- Windows PowerShell: `irm https://raw.githubusercontent.com/GofMan5/gof-lang/main/scripts/install.ps1 | iex`

Это хороший путь, если ты хочешь попробовать язык как пользователь.

### Путь B: запускать прямо из репозитория

Если ты развиваешь сам язык или хочешь самый свежий bootstrap-state:

```bash
cargo run -q -p gof-cli --bin gof -- run examples/hello_print.gof
```

Это правильный путь, если тебе важны compiler behavior, examples и tests.

Если тебе нужно просто писать и запускать программы на `gof`, не оставайся на Rust-пути.
Ставь `gof`, убеждайся, что он есть в `PATH`, и используй обычный CLI напрямую.

## Первая программа

Напиши так:

```gof doctest
fn main() -> int:
    print("hello from gof")
    return 42
```

Сохрани как `hello.gof` и запусти:

```bash
gof run hello.gof
```

### Что здесь реально происходит

- `print("hello from gof")` пишет текст в stdout
- `return 42` становится финальным значением программы в bootstrap CLI path

Это намеренное разделение. `print` — side effect. `return` — итог программы.

## Команды, которые реально нужно знать сначала

Форматирование:

```bash
gof fmt hello.gof
```

Проверка форматирования без переписывания:

```bash
gof fmt hello.gof --check
```

Запуск fixture suite:

```bash
gof test tests/fixtures
```

Сборка backend artifact:

```bash
gof build hello.gof
```

Сборка bootstrap-native executable:

```bash
gof build hello.gof --native
```

Если ты разрабатываешь `gof` прямо из репозитория и еще не ставил бинарь,
используй тот же CLI через префикс:

```bash
cargo run -q -p gof-cli --bin gof --
```

## Пойми build modes правильно

Это одно из мест, где очень легко выучить язык неправильно.

- `gof run` исполняет программу через bootstrap evaluator
- `gof build` по умолчанию делает структурированный backend artifact
- `gof build --native` делает реальный host executable

Но:

> `--native` пока не означает финальный direct codegen backend. Это bootstrap-native path.

## Нормальный порядок изучения

Лучше идти так:

1. [Обзор языка](./language-tour.html)
2. [Типы и данные](./types-and-data.html)
3. [Управление потоком](./control-flow.html)
4. [Модули и файлы](./modules-and-files.html)
5. [Базовая стандартная библиотека](./stdlib-baseline.html)
6. [Многопоточность](./concurrency.html)
7. [Инструменты и native build](./tooling-and-native-build.html)

## Что новички обычно понимают неправильно

> То, что язык пока маленький, не значит, что он примитивный.

Это значит, что проект сознательно строит чистое ядро языка, прежде чем
притворяться зрелой экосистемой.
