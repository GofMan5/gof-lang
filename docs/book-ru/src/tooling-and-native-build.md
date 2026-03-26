# Инструменты и native build

У `gof` уже есть единый CLI:

- `gof run`
- `gof build`
- `gof test`
- `gof fmt`
- `gof mod`
- `gof doc`
- `gof bench`

Это важно, потому что проект хочет быть не только читаемым языком, но и нормальным toolchain.

## Что означают команды сегодня

### `gof run`

Запускает программу через bootstrap execution path.

### `gof build`

По умолчанию:

```bash
gof build program.gof
```

выдает структурированный backend artifact.

### `gof build --native`

С:

```bash
gof build program.gof --native
```

CLI делает runnable host executable.

## Главное честное правило

Сегодня `--native` — это bootstrap-native path. Сгенерированный executable упаковывает bootstrap evaluator вокруг source program.

Это полезно, потому что:

- уже сейчас можно запускать native host executable
- install и packaging flow уже могут работать с реальным бинарем
- можно закалять build ergonomics до финального direct codegen backend

Но это еще не финальный backend.

## Что значит хороший tooling для `gof`

Проект целится в tooling, который:

- предсказуем
- кроссплатформен
- хорошо скриптуется
- явно говорит, что именно он производит

Поэтому книга всегда различает:

- evaluator run
- backend artifact build
- bootstrap-native executable build

## Сборка книги

Документация книги собирается через `mdBook`.

Английская и русская версии должны публиковаться вместе через GitHub Pages.

Локально английскую книгу можно собрать так:

```bash
mdbook build docs/book
```

Русскую так:

```bash
mdbook build docs/book-ru
```
