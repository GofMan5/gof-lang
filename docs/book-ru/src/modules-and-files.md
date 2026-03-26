# Модули и файлы

Bootstrap module system в `gof` маленький и строгий намеренно.

Это плюс, а не минус: язык пытается стабилизировать semantics до того, как
притворяться полноценной package ecosystem.

## Локальные импорты

`import name` сейчас ищет `name.gof` рядом с импортирующим файлом.

Пример:

```gof
import math

fn main() -> int:
    return square(9)
```

и `math.gof`:

```gof
fn square(x: int) -> int:
    return x * x
```

## Как ведет себя текущий bootstrap module graph

Сейчас он:

- работает для same-directory imports
- загружает файлы в один merged bootstrap module graph
- запрещает import cycles
- запрещает duplicate top-level functions
- запрещает duplicate top-level structs
- запрещает duplicate top-level enums
- запрещает конфликты имен между function / struct / enum

## Как лучше организовывать маленькие программы

Сейчас лучшая практика такая:

- держать связанные файлы в одной директории
- разносить helper logic по понятным модулям
- не строить слишком хитрые import trees
- держать top-level names уникальными и ясными

## Чего пока нет

Сейчас еще нет:

- package registry
- полноценного dependency resolution
- arbitrary install scripts
- большой namespacing system

Текущий module system надо воспринимать как:

> чистую модель локальной композиции, а не как уже готовую ecosystem story.
