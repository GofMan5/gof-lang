# Модули и файлы

Bootstrap module system в `gof` по-прежнему маленький и строгий намеренно.

Это плюс, а не минус: язык пытается стабилизировать semantics до того, как
притворяться полноценной package ecosystem.

## Локальные импорты

`import name` теперь ищет модуль в детерминированном порядке:

1. `name.gof` рядом с импортирующим файлом
2. `src/name.gof` внутри ближайшего package root с `gof.mod`
3. `src/lib.gof` из локальной зависимости, объявленной как `name = { path = "../dep" }`

Same-directory imports продолжают работать как раньше.

Package-root imports позволяют держать entrypoint в поддиректории, не теряя
плоскую module surface внутри пакета.

Пример пакета:

```text
app/
  gof.mod
  src/
    main.gof
    math.gof
```

с `gof.mod`:

```toml
module = "example/app"
edition = "2026"

[dependencies]
```

и `src/main.gof`:

```gof
import math

fn main() -> int:
    return square(9)
```

## Локальные path dependencies

Текущий package workflow также поддерживает локальные path dependencies.

Пример manifest у приложения:

```toml
module = "example/package_app"
edition = "2026"

[dependencies]
package_math = { path = "../package_math" }
```

Layout зависимости:

```text
package_math/
  gof.mod
  src/
    lib.gof
    ops.gof
```

`src/lib.gof` — это dependency entrypoint, который загружается для `import package_math`.

## Locking локального графа

Для manifest-backed packages теперь нужен committed `gof.lock`.

Создать или обновить его можно так:

```text
gof mod resolve --dir package_app
```

Правила:

- `gof.lock` — единственный lockfile format
- писать его умеет только `gof mod resolve`
- `gof run`, `gof build` и package-aware `gof test` требуют свежий lockfile для manifest-backed packages
- обычные правки `.gof` файлов сами по себе lockfile не протухают
- изменения `gof.mod` или dependency paths делают lockfile stale
- записи в lockfile сериализуются детерминированно и используют forward-slash relative paths

## Как ведет себя текущий bootstrap module graph

Сейчас он:

- работает для same-directory imports
- работает для package-root imports через ближайший `gof.mod`
- работает для локальных path dependencies через `[dependencies]`
- загружает файлы в один merged bootstrap module graph
- запрещает import cycles
- запрещает duplicate top-level functions
- запрещает duplicate top-level structs
- запрещает duplicate top-level enums
- запрещает конфликты top-level имен между functions, structs и enums

Эта строгость нужна, потому что constructors, type names и callable names должны
оставаться однозначными.

## Как лучше организовывать программы сейчас

Лучшая текущая практика такая:

- держать package root с `gof.mod`
- держать executable entrypoint в `src/main.gof`
- держать dependency-facing exports в `src/lib.gof`
- раскладывать helper logic по понятным модулям внутри `src/`
- не строить слишком хитрые import trees
- держать top-level names уникальными и осознанными

Например:

- `src/main.gof` для entrypoint logic
- `src/lib.gof` для экспортов, которыми пользуются другие пакеты
- `src/math.gof` для math helpers
- `src/models.gof` для `struct` и `enum`

## Чего пока нет

Сейчас еще нет:

- полноценного package registry
- published dependency resolution flow
- arbitrary install scripts
- большой namespacing system

Текущую module system надо воспринимать как:

> детерминированный локальный package workflow с lockfile, а не как уже готовую ecosystem story.
