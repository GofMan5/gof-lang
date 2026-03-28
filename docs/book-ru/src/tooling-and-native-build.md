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
упаковывает bootstrap evaluator вокруг source program.

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

## VS Code editor baseline

В репозитории есть и installable-расширение для VS Code в `tools/vscode-gof`.

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
с отдельным набором editor-only эвристик.

Локальная сборка пакета:

```bash
cd tools/vscode-gof
npm install
npm test
npm run package
```

После этого получается `.vsix`, который можно поставить через
`Extensions: Install from VSIX...`.

Если нужен нормальный one-click install для других пользователей, этот же пакет
нужно публиковать в VS Code Marketplace. Это уже реальный editor tooling, но
пока только editor baseline, а не финальный IDE/platform слой из M12.

Rolling prerelease `snapshot-main` тоже прикладывает готовый `.vsix`, так что
install-тестирование не зависит от публикации в Marketplace.

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
