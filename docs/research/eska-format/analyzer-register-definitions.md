# Переход из BSL к полям регистров

Здесь сохранён компонент на `1c95fc6`. Следующие
[saved file events](analyzer-watched-files.md) добавили инвалидацию ввода;
counts и замеры ниже относятся к register definitions.

`textDocument/definition` открывает измерение, ресурс или реквизит записи
регистра в текущем YAML. Для `Набор = РегистрыСведений.Rates.СоздатьНаборЗаписей();
Запись = Набор.Добавить(); Запись.Поле` выбирается дочерний объект именно `Rates`.
`Набор.Отбор.Измерение` ведёт к тому же измерению; ресурс через отбор не получает
ложную цель.

## Общая модель

Поддержаны записи регистров сведений, накопления, бухгалтерии и расчёта,
менеджер записи регистра сведений и измерения отбора. Тип receiver и
RegisterDimension/RegisterResource/RegisterAttribute из существующего
[HIR facade](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/hir-ty/src/field_enum.rs)
выбирают Dimension/Resource/Attribute. Ready каноническая модель и точный
parent/kind/name текущего документа подтверждают UUID. Прежний
[metadata locate](analyzer-metadata-locations.md) возвращает диапазон физического
YAML и его hash. Case folding и inferred alias используют прежние механизмы.

Набор записей сам по себе не получает переход к полю записи. Его поле `Отбор`
остаётся свойством платформы. Стандартные поля без собственного дочернего
YAML-объекта, например период, не получают выдуманный UUID. Поля `Структура`
также не превращаются в метаданные.

При переименовании измерения в editor YAML обе формы — запись и отбор —
перестают подтверждать прежнее имя. Новое имя открывает тот же UUID с текущими
координатами и hash. Invalid overlay очищает модель; close возвращает сохранённую.
Нет нового индекса, чтения XML или диска для координат. Языковое ядро,
dependencies, CLI options и config keys не изменены.

Params, Location и `eska.target` сохраняют форму
[member definitions](analyzer-member-definitions.md). Scope остаётся
ready-bsl-and-canonical-metadata, metadataTargets — typed-objects-and-members.
Snapshot, source hashes, coverage, resources, отмена и deadline общие.
Остальные receiver kinds и полная семантика ещё не объявлены.

## Проверки

Прошли 48 unit/file, 48 analyzer process и 9 checker process tests, все 19 tool
gates четырёх feature profiles, strict Clippy, Rustdoc и release. Root: 892 passed /
14 прежних ignored; minimal 164/3, project-cli 180/3, build-cli 199/3, все 15 gates
прошли. Тестовый пример покрывает все четыре вида регистров, три вида полей,
менеджер записи, отбор, одинаковые имена разных владельцев, rename/invalid/close
и текущую overlay-цель при отсутствии файла на диске. Ошибки назначения
receiver не маскируются совпадением имени.

На C1/C2/C3 в RU/EN прошли 16 сценариев: 320 подтверждённых definition,
320 независимых locate и 576 definition с отказами. Проверены saved/editor/close,
record/filter rename, pending/stale/invalid, structure/record-set/standard/filter-resource
refusals, alias, Unicode, BOM в BSL и non-BMP/CRLF. Точные UTF-16/byte ranges и
hashes совпали; machine traces совпали между локалями.

| Corpus | Виды регистров в реальной проверке | Saved запросы, мс | Peak RSS, KiB |
|---|---|---:|---:|
| C1 | сведений, накопления | 0,369–11,745 | 68848–69532 |
| C2 | сведений, накопления, бухгалтерии | 0,465–59,625 | 219680–220480 |
| C3 | сведений, накопления, бухгалтерии | 0,376–68,877 | 247128–248072 |

В корпусах нет регистров расчёта: для них подтверждены только library/process
tests на тестовом примере. Это ограниченный сценарий с явно прочитанным владельцем
и маленькими editor BSL. Первый запрос выполняет inference; следующие используют
текущую сессию. Замеры не являются cold-start SLA, полным анализом или heap/RSS
budget. Semantic complete остаётся false.

Четыре выбранных файла каждого сценария — manifest, configuration YAML,
owner YAML и saved BSL — сохранили исходные bytes. Остальные файлы отдельно
не хешировались. PATH пустой; normal exit не ждал stdin EOF; собственные
временные settings C2/C3 удалены. Платформа 1С и настоящий VS Code client
в этой проверке не запускались.

Подключённый bsl-analyzer-reference по точным reference IDs подтвердил
СоздатьМенеджерЗаписи, возвращающий тип менеджера записи, а также тип набора:
Добавить возвращает запись, Отбор — свойство набора. Legacy lookup без шаблонного
имени типа не нашёл карточки, поэтому отсутствие результата не трактовалось
как отсутствие API. Workspace MCP в сессии недоступен. Установленный
`bsl-analyzer-app 0.3.0` проверил все 84 уникальных BSL-фрагмента с профилем
ParseError: 84 file results, ноль диагностик, exit 0. Собственный временный
каталог фрагментов удалён. CLI сообщил degraded platform help; проверка синтаксиса
не требует описаний API и не заменяет семантику или запуск 1С.

Runtime graph остался 206 package names / 214 resolved versions;
release server — 48847528 bytes. [Машинный отчёт](analyzer-register-definitions-results.json)
содержит gates, явное наличие семейств в корпусах и замеры без локальных путей
или имён конфигураций.

EF-18 остаётся IN_PROGRESS: form data, predefined values, union receivers,
пространства общих модулей, query text, все IDE edits, полнота анализа,
общий memory budget, watcher и Explorer ещё не завершены.
