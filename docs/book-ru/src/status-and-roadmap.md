# Статус и roadmap

Лучший способ понять `gof` — всегда разделять три вещи:

- каким язык хочет стать
- что уже реально реализовано
- что еще bootstrap или намеренно неполно

Если смешать эти три слоя, можно либо недооценить проект, либо поверить в
обещания, которые еще не заработаны.

## Что уже реально есть

В текущем репозитории уже есть:

- typed functions
- structs, payload enums и методы
- исчерпывающий `match`
- lists, dicts и parameterized builtin types
- `Result[T, E]` и postfix `?`
- `assert`
- process, filesystem и path helpers
- JSON helpers и bootstrap HTTP GET
- channels, `close`, cancellation tokens и `select`
- `go` / `await`
- language-level `test fn`, typed `fixture(scope) fn`, shipped `testing` stdlib, snapshot-aware `gof test` и markdown doctests для explicit fences и whole-file plain examples
- реальный CLI и bootstrap-native build path

Этого уже достаточно, чтобы учить реальную семантику, запускать нетривиальные
examples и собирать первый baseline long-polling Telegram bot.

## Что еще в работе

Следующие крупные шаги:

- richer stdlib без semantic mud
- следующие testing slices: unified product harnesses, reporters, property/fuzz/stress и benchmark gates
- production-grade concurrency contracts
- package system hardening
- direct native code generation

Это не косметические milestones. Это слои, которые переводят `gof` из
«сильного bootstrap-языка» в «серьезный production-язык».

## Как правильно использовать roadmap

Используй `roadmap.md`, чтобы понимать:

- над каким milestone проект реально работает
- какие checkpoints уже закрыты
- что именно отложено намеренно

Roadmap — это инженерный truth source, а не маркетинговый текст.

Если нужен не только summary-status, а упорядоченная программа реализации по
областям, смотри `plans/roadmap/`. Тестовая платформа уже разложена туда
отдельным деревом файлов.

## Куда смотреть дальше

- `README.md` — публичный обзор проекта
- `roadmap.md` — статусы milestones
- `plans/roadmap/` — пофайловые планы развития
- `spec/` — точные контракты языка и diagnostics
- `examples/` — runnable source files
- эта книга — обучающий слой поверх всего этого
