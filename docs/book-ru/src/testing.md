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

## Контракт snapshots

Snapshots сделаны намеренно явными:

- лежат в `tests/snapshots/`
- путь вычисляется из source file path и вложенных `t.case(...)`
- обновляются только через `--update-snapshots`

Это сохраняет snapshot churn видимым и reviewable, вместо тихой перезаписи
артефактов при обычном `gof test`.

## Что еще не shipped

Этот срез уже полезный, но сознательно далек от финальной платформы.

Пока еще не shipped:

- явные fixture cleanup hooks поверх temp/env cleanup, который уже делает `TestContext`
- автоматический doctest для каждого обычного markdown-блока `gof`
- unified `tests/ui` и `tests/runtime` product harness под `gof test`
- JSON и JUnit reporters
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
