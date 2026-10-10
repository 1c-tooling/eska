# Переход из BSL к метаданным общего модуля

Здесь сохранён компонент на `8db020d`. Следующие
[переходы для составного типа](analyzer-union-definitions.md) расширили members;
counts и замеры ниже относятся к namespace definitions.

`textDocument/definition` на имени `Tools` в `Tools.Method()` открывает объявление
общего модуля в текущем YAML. Готового канонического владельца достаточно;
тело целевого BSL-модуля может оставаться непрочитанным. Это переход к namespace,
а не подтверждение существования `Method`.

## Общая модель

Для FreeName используется тип CommonModule из существующего
[HIR facade](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/hir/src/semantic_symbol.rs).
Ближайшее BSL-объявление проверяется без вычисления binding types. Локальная
переменная, параметр, метод и типизированный локальный alias сохраняют BSL-цель;
совпадение написания само по себе не создаёт метаданные.

Поддержан Root config facet. Существующий common-module index выбирает owner;
resolve_common_module подтверждает текущую каноническую модель. Ready owner
и прежний [metadata locate](analyzer-metadata-locations.md) возвращают native UUID,
physical YAML URI, origin, source hash, byte span и UTF-16 range.
Нового индекса, spelling fallback, чтения target BSL, XML или диска для координат нет.

Invalid и pending YAML не публикуют прежнюю цель. Editor YAML остаётся текущим
источником даже после удаления сохранённого файла; close возвращает saved input.
Изменение корневого Name нарушает объявление состава, закреплённое при открытии,
и делает owner unavailable. Это не поддержка live rename состава проекта.

Params, Location и `eska.target` сохраняют прежнюю форму. Scope остаётся
ready-bsl-and-canonical-metadata, metadataTargets — typed-objects-and-members.
Snapshot, source hashes, coverage, resources, отмена и deadline общие. CLI,
config keys, языковое ядро и dependencies не изменены.

## Проверки

Прошли 56 unit/file, 61 analyzer process и 9 checker process tests; все 19 tool
gates четырёх feature profiles, strict Clippy, Rustdoc и release. Minimal core:
44 unit/file tests. Root: 893 passed / 14 прежних ignored; minimal 165/3,
project-cli 181/3, build-cli 200/3; все 15 gates прошли.

На C1/C2/C3 в RU/EN прошли 12 сценариев: два non-global server common modules
на corpus, 72 подтверждённых namespace definition, 72 независимых locate,
48 BSL shadow/alias definition и 168 definition всего. Проверены mixed case,
Unicode, BOM, non-BMP/CRLF, saved/editor/close, file deletion, invalid/pending/stale,
сохранённая инвалидация и локальные объявления. Точные byte/UTF-16 ranges
и hashes совпали; machine traces совпали между локалями.

| Corpus | Подтверждённые запросы, мс | Максимальный RSS, KiB |
|---|---:|---:|
| C1 | 0,184–7,190 | 68988 |
| C2 | 0,184–55,336 | 216900 |
| C3 | 0,177–71,384 | 243456 |

Каждый сценарий использует собственную частичную копию с полным исходным
составом и пятью payloads: manifest, configuration YAML, target owner,
caller owner и caller BSL. Target BSL не копировался; module_text_bytes равнялся
длине caller BSL во всех подтверждениях. Native XML отсутствовал. Остальные
payloads намеренно отсутствовали, semantic complete остаётся false.
Пять выбранных оригинальных файлов сохранили bytes и не записывались;
остальные файлы отдельно не хешировались. Копии восстановлены, собственные
проекты удалены. PATH пустой; normal exit не ждал stdin EOF.

Workspace MCP в сессии недоступен. Установленный `bsl-analyzer-app 0.3.0`
проверил 18 уникальных BSL-фрагментов с профилем ParseError: 18 file results,
ноль диагностик, exit 0. Project settings в отдельном каталоге отсутствовали;
профиль задан явно через CLI. Каталог удалён. CLI сообщил degraded platform help;
это проверка синтаксиса, не API или выполнения в 1С.

Runtime graph совпал с предыдущим компонентом: 206 package names / 214 resolved
versions, без Git engine, LSP runtime или сетевого клиента. Существующий
gix-validate остаётся вспомогательной зависимостью проверки путей.
Release server — 49060232 bytes. [Машинный отчёт](analyzer-namespace-definitions-results.json)
содержит gates и замеры без локальных путей или имён конфигураций.

Платформа 1С и настоящий VS Code client здесь не запускались. Замеры не являются
SLA, полным анализом или общим heap/RSS budget. EF-18 остаётся IN_PROGRESS:
form data, predefined values, union receivers, query text, все IDE edits,
полнота анализа, общий memory budget, saved-input watcher и Explorer ещё в работе.
