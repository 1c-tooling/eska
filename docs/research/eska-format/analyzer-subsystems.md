# Текущие модели подсистем

Главный descriptor подсистемы подключён к общей сессии как
`CanonicalMetadata::Subsystem`. IDE и checker используют один адаптер текущего
YAML; Content и имена прямых дочерних подсистем публикуются одним значением.
Invalid/unavailable очищают прежнюю модель. Это продолжение
[регламентных заданий](analyzer-scheduled-jobs.md) в EF-18, без новых crates,
dependencies, команд или XML-экспорта.

## Граница модели

Адаптер сохраняет три поля закреплённой
[native model](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/subsystem.rs):
имя, упорядоченный Content и имена непосредственных дочерних подсистем.
Как в [XML-reader](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/xml_parser/subsystem.rs),
Content включает распознанные `MdoType` из ссылок `Type.Name`. Неизвестные виды,
UUID без префикса и повреждённые ссылки не создают рёбер. Порядок, повторы и хвост
имени после первой точки сохраняются. Имена прямых детей берутся из reference
с тем же owner, без смешивания с более глубокими уровнями.

Пустой Content допустим. Дубли свойства, scalar вместо record, foreign namespace
и неверная структура Item дают typed issue с текущим диапазоном Content.
Неполный descriptor не получает готовую модель. UUID проверяется в исходной
идентичности, хотя native Subsystem не хранит отдельного поля UUID.

В локальном ядре добавлены `Eq` и tracked resolver; XML-parser не менялся.
Resolver учитывает source revision, вид и имя. Недоступный, несовместимый или
ошибочный зарегистрированный slot не читает XML и не возвращает прежнюю модель.
Правка владельца не перевычисляет модель соседа и name index. Для неканонического
входа сохранён существующий путь XML-reader.

Listing передаёт имена в BSL completions `Метаданные.Подсистемы` /
`Metadata.Subsystems`. Экспериментальный Rust enum расширен; exhaustive matches
требуют нового варианта. JSON schemas и протокольные команды не изменились.

Подключён только главный descriptor физического YAML-владельца. Вложенные
descriptors доступны общей модели дерева, но не опубликованы как отдельные
значения ядра. `modeled_descriptors` остаётся равным одному; такой владелец
сохраняет `metadata_descriptors_unmodeled` и неполное semantic coverage.

## Проверки

48 unit и шесть файловых тестов адаптера, native 1 107 passed / четыре прежних
ignored, analysis 261 tests (119 unit/file, 128 analyzer, 14 checker) прошли.
Все шесть adapter, пять kernel, 19 tool и 15 root gates прошли: fmt/check,
strict Clippy/Rustdoc, feature matrix и release builds. Root — 895/14, minimal —
165/3, project-cli — 182/3, build-cli — 201/3. Rust 1.98.1; версии и зависимости
не менялись. Проверены 322 исходных native hashes из provenance.

| Корпус | Главных моделей | Вложенных descriptors без модели | YAML/XML/control hashes |
| --- | ---: | ---: | ---: |
| C1 | 13 | 92 | 28 |
| C2 | 76 | 698 | 154 |
| C3 | 108 | 985 | 218 |

Read-only пример `project_subsystems` сравнил все 197 главных моделей с
независимым native reader; полное равенство подтверждено. Проверенные bytes
сохранены. PATH пустой, XML не материализовался, платформа не запускалась.

Шесть IDE-процессов RU/EN на собственных частичных копиях C1/C2/C3 прочитали все
главные подсистемы и вернули 13/76/108 имён в completion. Проверены current Content,
invalid Item, повреждённый YAML, editor authority при подготовке semantic inputs
и close. Текущие hashes и UTF-16 ranges с non-BMP/CRLF совпали; machine trace RU/EN
одинаков. Bytes 17/80/112 оригинальных файлов и их копий сохранены, временные проекты
удалены. Semantic coverage остаётся false: прочие payloads намеренно отсутствуют,
а nested descriptors не моделируются. Peak RSS — 76 880–301 760 KiB,
сценарий — 0,49–18,68 s. Это локальный замер без общего memory budget.

Полный saved C1 checker в RU/EN проверил все 1 975 модулей с прежним SelfAssign
профилем. Все 26 findings, settings hash, source quota и coverage сохранились.
13 unsupported-subsystem issues заменились семью явными отказами по nested
descriptors: metadata issues сократились с 1 518 до 1 512. Остальные 1 505 причин
не изменились. JSON одинаков, bytes всех 7 829 исходных файлов сохранены.
Exit 2 остаётся из-за неполноты моделей.

Workspace MCP недоступен; CLI fallback `bsl-analyzer-app` 0.3.0 проверил три
уникальных новых BSL-тела: ноль ParseError, три SelfAssign, ноль failed files.
Reference MCP подтвердил коллекцию Subsystems в серверном контексте; описание API
отсутствовало, platform help degraded. Настоящий IDE-клиент и 1С runtime не запускались.
Normal graph прежний 206/214; release analyzer/checker — 50 245 104 / 46 309 816 bytes.
[Машинный отчёт](analyzer-subsystems-results.json) содержит обезличенные результаты.

## Ограничения

Nested canonical inputs, прочие модели, полная семантика, общий heap/RSS budget,
остальные IDE edits и Explorer остаются в работе. Исходная квота не является
лимитом памяти процесса; EF-18 остаётся `IN_PROGRESS`.
