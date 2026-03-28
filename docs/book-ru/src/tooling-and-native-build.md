# Инструменты и native build

У `gof` уже есть единый CLI:

- `gof run`
- `gof check`
- `gof build`
- `gof test`
- `gof fmt`
- `gof mod`
- `gof doc`
- `gof bench`

Это важно, потому что проект хочет быть не только читаемым языком, но и
нормальным toolchain.

## Что означают команды сегодня

### `gof run`

Запускает программу через bootstrap execution path.

Теперь у `gof run` есть и script-first watch-режим:

```bash
gof run --watch program.gof -- --example arg
```

Текущий контракт watch-режима специально сделан простым и честным:

- initial run стартует сразу
- одновременно выполняется только один run
- серия быстрых правок схлопывается через небольшой debounce
- compile/runtime ошибки не убивают loop
- loop использует тот же execution path, что и обычный `gof run`

Для executable manifest-backed packages watch-режим отслеживает:

- `src/` корневого пакета
- корневые `gof.mod` и `gof.lock`
- `src/` всех locked local dependencies
- `gof.mod` всех locked local dependencies

Если package graph становится stale, loop показывает текущую проблему
lockfile/manifest и ждет следующего изменения вместо тихого завершения.

### `gof check`

С:

```bash
gof check program.gof
```

CLI делает только компиляционную проверку без запуска программы.

Для editor tooling и automation есть и machine-readable path:

```bash
gof check program.gof --json --stdin
```

Именно этот контракт использует VS Code extension для compiler-backed
diagnostics.

### `gof test`

Сегодня package-aware `gof test` имеет явный split contract:

- executable package targets (`src/main.gof`) делают compile plus execute smoke
- library package targets (`src/lib.gof`) пока остаются compile-only

Это держит команду честной для runnable packages и не притворяется, что у
языка уже есть отдельная встроенная test surface для библиотек.

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

Сегодня `--native` это bootstrap-native path. Сгенерированный executable
упаковывает bootstrap evaluator вокруг deterministic embedded source bundle.

Это полезно, потому что:

- уже сейчас можно запускать native host executable
- install и packaging flow уже могут работать с реальным бинарем
- можно закалять build ergonomics до финального direct codegen backend
- reachable same-directory imports и manifest-backed local package imports
  продолжают работать даже если binary запущен из другой working directory

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

Что этот срез пока не дает:

- REPL
- shebang/direct executable script UX
- packaging/install flows для scripts
- hot reload
- incremental compilation reuse внутри watch-режима

## VS Code editor baseline

В репозитории есть и расширение для VS Code в `tools/vscode-gof`, а основной
пользовательский install path теперь идет через Marketplace:

- [gof Programming Language](https://marketplace.visualstudio.com/items?itemName=gofman5.gof-language)

Сейчас оно покрывает:

- ассоциацию файлов `.gof`
- syntax highlighting
- стартовые snippets для модулей, функций, enum, struct, `select` и task join
- compiler-backed diagnostics через `gof check --json --stdin`
- комментарии `#`
- пары скобок
- отступы для блоков, заканчивающихся `:`
- icon и packaging metadata для публикации

Оно пока не дает:

- LSP
- debugger
- profiler
- semantic rename или go-to-definition

Порядок разрешения toolchain для diagnostics такой:

1. `gof.toolchain.path`
2. repo-local cargo fallback внутри репозитория `gof`
3. `gof` в `PATH`

Это держит editor diagnostics синхронизированными с реальным компилятором, а не
с отдельным набором editor-only эвристик. Extension теперь также держит
diagnostics single-flight на документ и отменяет stale compiler runs во время
частых edits и config refresh.

Локальная сборка пакета:

```bash
cd tools/vscode-gof
npm install
npm test
npm run package
```

После этого packaged artifact остается прямо в `tools/vscode-gof/` для ручной
загрузки maintainer-ом в Marketplace. Snapshot release больше не прикладывает
editor package по умолчанию.

Это уже реальный editor tooling, но пока только editor baseline, а не
финальный IDE/platform слой из M12.

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
