# Переход к общему полю составного типа

Результат компонента `649c3ce` от 2026-10-10. Последующая
[регистрация saved inputs](analyzer-saved-watcher.md) расширяет file-watching protocol.

`textDocument/definition` открывает текущий YAML, если все содержательные варианты
составного типа выбирают одно объявление. Например, объект и ссылка одного
справочника ведут к одному реквизиту. Null и Неопределено не имеют собственного
объявления поля и пропускаются; состав только из этих вариантов не даёт цели.

## Общая модель

Используется TypeKind::Union из прежнего HIR facade. Каноническое представление
[ядра типов](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-types/src/intern.rs)
раскрывает вложенные unions и нормализует arms. Wrapper проверяет каждый
содержательный arm отдельно через уже существующие правила typed members;
он не выбирает первое поле из объединённого списка автодополнения.

Должны совпасть тип owner, его каноническое имя и весь путь kind/name дочерних
метаданных. Сравнение имён использует прежнее Unicode case folding. Разные
владельцы с одинаковым названием поля, структура и число не дают произвольной
цели. Ready каноническая модель и прежний metadata locate подтверждают native UUID
и точные координаты физического YAML в текущей revision.

Нового inference, индекса, кэша, чтения XML или target BSL нет. Return contract
локальной функции уже даёт ядру составной тип; wrapper использует этот факт.
Обычный параметр без recorded type и `?()` с различающимися типами сейчас могут
остаться unknown. Это ограничение inference, не поддержка всех способов вывода
составного типа. Нормализация Unknown/Any принадлежит ядру; wrapper не
восстанавливает отброшенные им варианты и не подтверждает annotations как runtime facts.

Invalid/pending owner очищает прежние цели всех arms. Editor YAML сохраняет
приоритет после удаления файла; close возвращает saved input. Scope,
metadataTargets, Params, Location и `eska.target` сохраняют прежнюю форму.
Неоднозначный union не возвращает несколько Locations: при неполном составе
остаётся navigation_incomplete. Snapshot, hashes, отмена, deadline и resources общие.

## Проверки

Прошли 59 unit/file, 63 analyzer process и 9 checker process tests; все 19 tool
gates четырёх feature profiles, strict Clippy, Rustdoc и release. Minimal core:
47 unit/file tests. Root: 893 passed / 14 прежних ignored; minimal 165/3,
project-cli 181/3, build-cli 200/3; все 15 gates прошли.
Library tests отдельно подтверждают реальный TypeKind::Union в inference,
совпадение owner/child, refusal одинаковых имён разных владельцев и rename/invalid/close.
Process tests проверяют оба порядка ambiguous arms и совпадение machine fields
между локалями.

На C1/C2/C3 в RU/EN прошли шесть сценариев: 48 подтверждённых definition,
48 независимых locate, 48 отказов, 96 definition всего. Проверены объект/ссылка,
nullable arms, inferred alias, structure/primitive/nullable-only/unknown ternary,
saved/editor/close, invalid/pending/stale, удаление файла, file events, BOM,
Unicode и non-BMP/CRLF. Точные byte/UTF-16 ranges и hashes совпали; locale traces
совпали после нормализации opaque IDs и пути собственного временного проекта.

| Corpus | Подтверждённые запросы, мс | Максимальный RSS, KiB |
|---|---:|---:|
| C1 | 0,390–0,953 | 77504 |
| C2 | 0,351–0,803 | 218204 |
| C3 | 0,305–0,997 | 245012 |

Это короткие queries после diagnostic/pending checkpoints в той же сессии,
не cold-start или SLA. Каждый сценарий использует собственную частичную копию
с полным исходным составом и пятью payloads: manifest, configuration YAML,
target owner, caller owner и caller BSL. Остальные payloads намеренно отсутствовали,
semantic complete остаётся false. Native XML отсутствовал; module_text_bytes
равнялся текущему caller BSL. Пять выбранных оригинальных файлов сохранили bytes
и не записывались; остальные файлы отдельно не хешировались. Копии восстановлены,
собственные проекты удалены. PATH пустой; normal exit не ждал stdin EOF.

Workspace MCP недоступен. Установленный `bsl-analyzer-app 0.3.0` проверил
16 уникальных BSL-фрагментов с профилем ParseError: 16 file results, ноль
диагностик, exit 0. Project settings в отдельном каталоге отсутствовали;
профиль задан явно через CLI. Каталог удалён. CLI сообщил degraded platform help;
это проверка синтаксиса, не API или выполнения в 1С.

Runtime graph совпал с предыдущим компонентом: 206 package names / 214 resolved
versions, без Git engine, LSP runtime или сетевого клиента. Существующий
gix-validate остаётся вспомогательной зависимостью проверки путей.
Release server — 49069224 bytes. [Машинный отчёт](analyzer-union-definitions-results.json)
содержит gates и ограниченные замеры без локальных путей или имён конфигураций.

Платформа 1С и настоящий VS Code client здесь не запускались. Общий heap/RSS
budget не подтверждён. EF-18 остаётся IN_PROGRESS: полная модель form data,
predefined values, query text, все IDE edits, полнота inference/анализа,
общий memory budget, saved-input watcher и Explorer ещё в работе.
