# Введение

<section class="docs-home-hero">
  <div class="docs-kicker">Книга gof</div>
  <h1 class="docs-home-title">Изучай язык по правильной модели мышления, а не по догадкам.</h1>
  <p class="docs-home-deck">
    <code>gof</code> стремится к читаемости Python, concurrency-модели уровня Go и
    инженерной строгости Rust. Важен не сам слоган, а то, что язык проектируется
    так, чтобы оставаться оптимизируемым, явным и нормально обучаемым с самого начала.
  </p>
  <div class="docs-pill-row">
    <div class="docs-pill"><strong>Читаемо</strong><br>низкая церемониальность, синтаксис на отступах</div>
    <div class="docs-pill"><strong>Явно</strong><br>типовые контракты, видимая мутабельность, честный control flow</div>
    <div class="docs-pill"><strong>Параллельно</strong><br><code>go</code>, <code>await</code>, каналы и <code>select</code></div>
    <div class="docs-pill"><strong>Честно</strong><br>bootstrap-ограничения называются bootstrap-ограничениями</div>
  </div>
  <div class="docs-card-grid">
    <a class="docs-card" href="./getting-started.html">
      <span class="docs-card__eyebrow">Старт</span>
      <span class="docs-card__title">Быстрый старт</span>
      <span class="docs-card__body">Установка, первый запуск и минимальный рабочий цикл без лишней воды.</span>
    </a>
    <a class="docs-card" href="./language-tour.html">
      <span class="docs-card__eyebrow">Модель языка</span>
      <span class="docs-card__title">Обзор языка</span>
      <span class="docs-card__body">Сначала пойми, каким язык пытается быть, а потом уже заучивай surface syntax.</span>
    </a>
    <a class="docs-card" href="./concurrency.html">
      <span class="docs-card__eyebrow">Суть gof</span>
      <span class="docs-card__title">Многопоточность</span>
      <span class="docs-card__body">Текущий baseline задач, каналов и <code>select</code> с честным описанием ограничений.</span>
    </a>
  </div>
  <div class="docs-inline-note">
    Главное правило: книга объясняет, spec фиксирует, examples доказывают, что реально работает сегодня.
  </div>
</section>

## Каким `gof` пытается быть

`gof` не пытается повторять внутренности Python. Он пытается сохранить плюсы
читаемости Python, но при этом оставить путь к нативной производительности,
предсказуемой многопоточности и долгосрочно поддерживаемой архитектуре.

Отсюда главный принцип:

> Если "похоже на Python" конфликтует с safety, оптимизируемостью или ясной
> семантикой, `gof` выбирает ясную семантику.

## Для чего эта книга

Это основной путь обучения `gof`.

Используй ее, если тебе нужны:

- объяснение текущего языка человеческим языком
- примеры, которые совпадают с реальной реализацией
- правильная картина того, что уже есть, а что еще bootstrap

## Для чего эта книга не предназначена

Это не формальная спецификация языка. Для точных правил смотри:

- `spec/language-v1.md`
- `spec/diagnostics.md`
- `rfcs/`

Книга учит. Спецификация определяет контракт. Примеры держат их в честном состоянии.

## Как правильно учить `gof`

Не учи `gof` как набор ключевых слов.

Учить надо в таком порядке:

1. что означают значения и биндинги
2. какие типовые контракты уже реальны
3. как устроен control flow
4. как моделируются данные
5. как сегодня ведут себя задачи, каналы и `select`
6. что реально делает toolchain

## Что реально есть уже сейчас

Сейчас в репозитории уже есть:

- функции и явные return contracts
- локальные импорты по соседним файлам и manifest-resolved local packages с детерминированным lockfile
- `struct`
- `enum`
- исчерпывающий `match`
- методы
- списки и словари
- `assert`
- файловый I/O
- `go` / `await`
- каналы и `select`
- `gof build --native`

Но часть этого все еще bootstrap-уровня:

- native build пока оборачивает bootstrap evaluator
- stdlib намеренно маленькая
- package system пока ограничена локальными пакетами без registry и solver
- concurrency semantics еще не production-grade

## Чего не надо предполагать

Не надо автоматически думать, что в `gof` уже есть:

- совместимость с Python
- зрелая package ecosystem
- финальная семантика каналов
- direct native code generation
- большая стандартная библиотека

Книга будет всегда предпочитать точное "еще нет" красивому, но ложному "почти уже да".
