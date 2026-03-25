<div align="center">

# gof

### Язык программирования с читаемостью Python, архитектурной строгостью системного языка и курсом на нативную производительность

<p>
  <strong>Статус:</strong> bootstrap-этап, активная разработка<br/>
  <strong>Курс:</strong> AOT-компиляция, строгий compiler pipeline, надежный runtime, развитие без костылей
</p>

</div>

---

## Что такое `gof`

`gof` создается как новый язык программирования, в котором:

- код читается легко и быстро
- синтаксис остается компактным и человеческим
- поведение языка контролируется строгой архитектурой
- эволюция языка идет через спецификации, тесты, RFC и ADR, а не через хаотичные хаки

Идея простая: взять ощущение легкости, которое нравится в Python, и совместить его с инженерной дисциплиной, предсказуемостью и скоростным потенциалом системного стека.

## Ключевой принцип

> `gof` не пытается быть Python-совместимой средой исполнения.  
> Он берет понятный стиль кода, но строится как отдельный язык с собственной моделью типов, компиляции и runtime.

---

## Состояние проекта на 2026

Сейчас в репозитории уже не пустой каркас, а рабочий bootstrap языка и toolchain.

### Уже реализовано

| Область | Что есть сейчас |
|---|---|
| CLI | `gof build`, `gof run`, `gof test`, `gof fmt`, `gof mod init`, `gof doc`, `gof bench` |
| Лексер | чувствительность к отступам, `INDENT` / `DEDENT`, базовые токены языка |
| Парсер | функции, параметры, биндинги, присваивания, вызовы функций, арифметика |
| Семантика | проверки неизвестных локалов, immutable reassignment, unknown function, wrong arity, duplicate binding, bool conditions |
| Formatter | детерминированное форматирование bootstrap-подмножества языка |
| Pipeline | `Lexer -> CST -> AST -> HIR -> Typed HIR -> MIR -> SSA -> backend artifact` |
| Исполнение | bootstrap evaluator для запуска программ через `gof run`, включая `if/else` и `while` |
| Тесты | unit, integration, fixture-based conformance, benchmark harness |

### Что пока еще не реализовано

| Область | Статус |
|---|---|
| Нативный machine code backend | еще нет |
| Импорты и модульная система как рабочая user-facing фича | еще нет |
| `struct`, `enum`, `protocol`, `match`, `async/await`, `select`, `defer`, `unsafe` | зарезервированы в направлении языка, но пока не реализованы |
| Реальный stdlib | еще нет |
| Полноценный package resolver и registry | еще нет |
| Производственный runtime с GC/scheduler/FFI | еще нет |

Важно: `gof build` сейчас выпускает **SSA JSON artifact**, а не финальный нативный бинарь. Это осознанный этап развития, а не имитация готового backend.

---

## Как выглядит код на `gof`

```gof
fn add(a, b):
    return a + b

fn main():
    base = 40
    mut total = add(base, 1)
    total = total + 1
    return total
```

При запуске:

```text
42
```

---

## Что умеет язык прямо сейчас

Текущее bootstrap-подмножество поддерживает:

- top-level `fn`
- параметры функций
- блоки через отступы
- `return`
- `if` / `else`
- `while`
- immutable binding через `name = expr`
- mutable binding через `mut name = expr`
- повторное присваивание только mutable-переменным
- целочисленные литералы
- булевы литералы `true` / `false`
- строковые литералы
- идентификаторы
- `+`, `-`, `*`
- сравнения `==`, `!=`, `<`, `<=`, `>`, `>=`
- вызовы top-level функций по имени

### Правила биндингов

- `name = expr` создает новую immutable-переменную, если такого имени еще нет
- `mut name = expr` создает mutable-переменную
- попытка изменить immutable binding приводит к диагностике компилятора
- вызовы функций в bootstrap-режиме разрешены только для top-level функций

---

## Быстрый старт

### Требования

- Rust toolchain, совместимый с [`rust-toolchain.toml`](./rust-toolchain.toml)

### Запуск примера

```bash
cargo run -q -p gof-cli --bin gof -- run tests/fixtures/pass/hello.gof
```

### Сборка SSA artifact

```bash
cargo run -q -p gof-cli --bin gof -- build tests/fixtures/pass/hello.gof
```

### Форматирование файла

```bash
cargo run -q -p gof-cli --bin gof -- fmt path/to/file.gof
```

### Проверка форматирования без перезаписи

```bash
cargo run -q -p gof-cli --bin gof -- fmt path/to/file.gof --check
```

### Прогон fixture-набора языка

```bash
cargo run -q -p gof-cli --bin gof -- test tests/fixtures
```

### Инициализация модуля

```bash
cargo run -q -p gof-cli --bin gof -- mod init example/app --dir .
```

### Полный прогон тестов

```bash
cargo test --workspace
```

### Сборка benchmark harness

```bash
cargo bench -p gof-bench --no-run
```

---

## CLI

Текущая командная поверхность:

- `gof build <file.gof>`
- `gof run <file.gof>`
- `gof test [fixtures-dir]`
- `gof fmt <file.gof> [--check]`
- `gof mod init <module> [--edition <edition>] [--dir <path>]`
- `gof doc`
- `gof bench`

---

## Архитектура компилятора

`gof` строится не вокруг одной монолитной фазы, а через строгую последовательность представлений:

1. `Lexer`
2. `CST`
3. `AST`
4. `HIR`
5. `Typed HIR`
6. `MIR`
7. `SSA`
8. `Backend Artifact`

Это сделано намеренно. Такой pipeline:

- изолирует ответственность каждой стадии
- не дает синтаксическим упрощениям протекать в lower-level архитектуру
- позволяет развивать оптимизации без разрушения frontend
- защищает проект от типичного сценария "временно захардкодим, потом перепишем"

---

## Диагностика

Компилятор уже выдает детерминированные диагностики с кодами и fix-it hints.

Текущие bootstrap-диагностики включают:

- неизвестный локальный binding
- попытку изменить immutable binding
- вызов неизвестной функции
- неверное число аргументов
- повторное объявление binding в одной функции
- небулевы условия в `if` и `while`

См.:

- [`spec/language-v1.md`](./spec/language-v1.md)
- [`spec/diagnostics.md`](./spec/diagnostics.md)

---

## Структура репозитория

| Путь | Назначение |
|---|---|
| `spec/` | спецификации языка, editions, diagnostics, package contracts |
| `rfcs/` | предложения, меняющие публичное поведение |
| `adrs/` | архитектурные решения по компилятору, runtime и границам проекта |
| `compiler/` | компилятор и все его внутренние фазы |
| `runtime/` | runtime contracts и будущее runtime-ядро |
| `stdlib/` | roadmap стандартной библиотеки |
| `tools/` | пользовательские инструменты, включая CLI `gof` |
| `tests/` | conformance fixtures и integration tests |
| `benchmarks/` | performance baseline и benchmark harness |

---

## Правила развития проекта

Проект изначально строится жестко и дисциплинированно.

### Базовые правила

- ни одна публичная языковая фича не должна появляться без обновления `spec/`
- нельзя ломать семантику без тестов и conformance fixtures
- нельзя хранить undocumented IR invariants
- нельзя тащить hidden feature flags в публичное поведение
- нельзя размазывать архитектурные решения по "временным" обходным путям
- каждый новый исходный файл должен иметь тесты, fixtures или оба слоя сразу
- цель проекта для deterministic compiler/runtime code: 100% line и branch coverage
- деградации производительности считаются блокером до объяснения и исправления
- скорость, стабильность и качество оптимизаций выше удобной халтуры

### Если меняется язык, обычно должны измениться и эти области

- `spec/`
- `rfcs/`
- `adrs/`
- `tests/fixtures/`
- unit/integration tests компилятора

---

## Roadmap 2026

Следующие большие шаги для `gof`:

- imports и module loading
- `if`, циклы и расширение statement/expression surface
- более сильный type inference
- typed bindings и richer semantic analysis
- реальный package resolver
- backend ниже уровня SSA JSON
- подъем runtime и stdlib
- дальнейшее движение к AOT-native исполнению

---

## Проверенный baseline

Текущее состояние репозитория уже подтверждается:

- unit-тестами компилятора
- CLI integration tests
- fixture-based conformance tests
- formatter checks
- сборкой benchmark harness

Это и есть минимальный качественный фундамент, на котором можно дальше строить настоящий язык, а не бесконечный набор идей без реализации.
