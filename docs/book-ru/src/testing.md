# Тестирование

В `gof` появился первый shipped-срез настоящей test-platform.

Это еще не финальная система тестирования, а первый цельный baseline, в
котором пользовательский код и код самого репозитория используют один и тот же
языковой тестовый surface, а не делают вид, что `assert(...)` уже заменяет
framework.

## Что уже есть

Сейчас shipped baseline включает:

- `test fn`
- `fixture(test) fn` и `fixture(module) fn`
- reserved stdlib import `testing`
- типизированный `TestContext`
- детерминированный discovery через `*_test.gof` и `tests/**/*.gof`
- snapshots в `tests/snapshots/`
- artifact-backed product fixtures в `tests/ui`, `tests/runtime` и `tests/runtime-fail`
- optional fixture cleanup hooks через receiver method `cleanup()`
- opt-in markdown doctests через `gof test --docs`

Пример:

```gof doctest no_run
import testing

test fn sums_are_stable(t: TestContext):
    case = t.case("small-sum")
    case.equal(2 + 3, 5, "expected deterministic integer addition")

test fn result_paths_stay_explicit(t: TestContext) -> Result[unit, RuntimeError]:
    parsed = parse_int("7")?
    return Result.Ok(t.equal(parsed, 7, "expected parse_int to preserve value"))
```

Запуск:

```text
gof test examples/testing_baseline
gof test --list examples/testing_baseline
```

## `TestContext`

В shipped `testing` module сейчас есть базовые helper-ы:

- `t.fail(message)`
- `t.equal(actual, expected, message)`
- `t.not_equal(actual, expected, message)`
- `t.true(condition, message)`
- `t.false(condition, message)`
- `t.ok(result, message)`
- `t.err(result, message)`
- `t.match_snapshot(name, value)`
- `t.case(name)`
- `t.temp_dir()`
- `t.temp_file(prefix)`
- `t.env(name, value)`
- `t.skip(message)`
- `t.todo(message)`

`TempDir.path()` и `TempFile.path()` возвращают реальный host path для временных
ресурсов, созданных runner-ом.

## Typed fixtures

Fixtures в `gof` сделаны как явные top-level функции, которые подают typed
зависимости в tests и другие fixtures.

```gof doctest no_run
import testing

fixture(module) fn shared_total() -> int:
    return 41

fixture(test) fn scratch_dir(t: TestContext) -> TempDir:
    return t.temp_dir()

test fn uses_fixtures(shared_total: int, scratch_dir: TempDir, t: TestContext):
    t.equal(shared_total, 41, "expected cached module fixture value")
    t.true(exists(scratch_dir.path()), "expected test fixture temp dir")
```

Текущие правила fixtures:

- `fixture(module)` кэширует одно значение на test file в рамках одного `gof test`
- `fixture(test)` создает одно значение на каждый test case
- dependency injection резолвится по имени параметра и совместимому типу
- fixture обязана явно объявлять return type и сейчас может возвращать только
  `Type` или `Result[Type, RuntimeError]`
- только `fixture(test)` может запрашивать `t: TestContext`
- fixture value может объявить optional receiver method
    `cleanup() -> unit | Result[unit, RuntimeError]`
- test-scoped cleanup запускается после тела теста, но до temp/env teardown из
    `TestContext`; module-scoped cleanup выполняется один раз после завершения файла

## Fixture cleanup

Возвращаемое fixture-value может явно описать teardown через receiver method с
именем `cleanup`.

```gof doctest no_run
import testing

struct ScratchDir:
        path: string

fn ScratchDir.cleanup(self: ScratchDir) -> Result[unit, RuntimeError]:
    return write_file(path_join(self.path, "teardown.log"), "cleanup ran")

fixture(test) fn scratch(t: TestContext) -> ScratchDir:
        dir = t.temp_dir()
        return ScratchDir(dir.path())
```

Это сохраняет lifecycle явным без второго callback DSL: teardown живет рядом с
типом, который fixture возвращает, runner вызывает hook в обратном порядке
зависимостей, а test-scoped cleanup успевает отработать до удаления temp
ресурсов и восстановления env overrides из `TestContext`.

## Контракт snapshots

Snapshots сделаны намеренно явными:

- лежат в `tests/snapshots/`
- путь вычисляется из source file path и вложенных `t.case(...)`
- обновляются только через `--update-snapshots`

Это сохраняет snapshot churn видимым и reviewable, вместо тихой перезаписи
артефактов при обычном `gof test`.

## Product fixtures

`gof test` теперь же ведет и repository-style product harness paths:

- `tests/ui/*.gof`: файл обязан падать на compilation
- `tests/runtime/*.gof`: файл обязан успешно выполняться
- `tests/runtime-fail/*.gof`: файл обязан скомпилироваться и упасть уже на runtime

Для этих путей используются явные artifact-файлы:

- `.diag` для diagnostic codes
- `.stdout` для CLI-visible stdout
- `.stderr` для отрендеренных diagnostics
- `.exit` для ожидаемого harness exit status

Обновление делается явно:

```text
gof test --update-snapshots path/to/project
```

Так compile/runtime product fixtures теперь живут под тем же CLI entrypoint,
что и language-level tests, вместо разъезда по отдельным одноразовым harness-скриптам.

## JSON reporter

`gof test --json` теперь выводит один machine-readable report в stdout со
stable schema `gof.test.report/v1`.

Сейчас этот report включает:

- нормализованные input paths и активные CLI options
- summary counts и общее wall-clock duration
- per-target events для language tests, compile failures, doctests, fixtures и
    package targets
- стабильные ids, source paths, captured stdout/stderr и structured diagnostics

Это дает CI, editor tooling и automation один и тот же честный контракт runner-а,
вместо screen-scraping человекочитаемого вывода.

## JUnit reporter

`gof test --junit` теперь выводит один JUnit/xUnit XML document в stdout.

Текущий mapping:

- каждый выполненный target становится `<testcase>` со stable test id в `name`
- нормализованный source path сохраняется через `classname` и `file`
- failures рендерятся как `<failure>`, skips как `<skipped>`, а `todo` как
    `<skipped type="todo">`
- captured stdout/stderr остаются видимыми через `<system-out>` и `<system-err>`

Это дает CI-системам обычный XML path без промежуточного перевода из JSON schema.

## Exit codes

`gof test` теперь резервирует два явных ненулевых exit code:

- `10`: честно упал хотя бы один найденный test target
- `11`: сам harness не смог честно завершить discovery, execution или
    reporting

Это позволяет CI отличать регрессии пользовательского кода от поломки запуска
или состояния harness-а без эвристического разбора stderr.

## Что еще не shipped

Этот срез уже полезный, но сознательно далек от финальной платформы.

Пока еще не shipped:

- автоматический doctest для каждого обычного markdown-блока `gof`
- property testing
- fuzzing
- concurrency stress
- benchmark integration
- параллельный запуск по умолчанию

Эти следующие слои зафиксированы в:

- [`roadmap.md`](../../../roadmap.md)
- [`plans/roadmap/20-testing-platform/00-overview.md`](../../../plans/roadmap/20-testing-platform/00-overview.md)

## Opt-in doctests

`gof test --docs` теперь умеет явный opt-in baseline для markdown fences:

- ```` ```gof doctest ````: compile + run
- ```` ```gof doctest no_run ````: только compile
- ```` ```gof doctest compile_fail ````: ожидаем compile failure
- ```` ```gof doctest runtime_fail ````: ожидаем runtime failure

Это позволяет честно проверять runnable примеры, не делая вид, что каждый
старый учебный фрагмент уже автоматически готов к doctest execution.
