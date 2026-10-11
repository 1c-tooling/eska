# Текущие регламентные задания

`ScheduledJob` подключён к общей сессии как `CanonicalMetadata::ScheduledJob`.
IDE и checker используют тот же адаптер текущего YAML. MethodName, Predefined
и Use обновляются одним каноническим значением; invalid/unavailable очищают
прежнюю модель. Новых crates, dependencies, XML-экспорта или протокольных команд нет.
Это продолжение [канонических подписок](analyzer-event-subscriptions.md) в EF-18.

## Граница модели

Адаптер сохраняет пять полей закреплённой
[native model](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/scheduled_job.rs):
UUID, имя, MethodName, Predefined и Use. Отсутствующие флаги становятся false,
отсутствующий MethodName — пустой строкой, как в
[XML-reader](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/xml_parser/scheduled_job.rs).
Явные флаги принимают только `true`/`false`; неверные значения, дубли и record
вместо скаляра дают typed issue с текущими координатами свойства. Foreign namespace
не подменяет одноимённое свойство схемы. Расписание и остальные свойства сохраняются
в общей модели проекта, но отсутствуют в этой BSL-модели ядра.

Локальная модель получает `Eq` и `from_parts`; нативный XML-parser не меняется.
Resolver учитывает source revision зарегистрированного slot, проверяет вид и имя.
Недоступный, несовместимый или ошибочный slot не читает XML и не использует старое
значение. Соседнее задание и name index не перевычисляются из-за чужой правки.
Для неканонического входа сохранён прежний путь XML-reader.

Устойчивый listing передаёт имена в существующие BSL completions коллекции
`Метаданные.РегламентныеЗадания` / `Metadata.ScheduledJobs`. Экспериментальный Rust
enum расширен; exhaustive matches прежней версии требуют нового варианта.
Схемы JSON и команды не менялись.

## Проверки

44 unit и шесть файловых тестов адаптера; native 1 106 passed / четыре прежних
ignored; analysis 259 tests (118 unit/file, 127 analyzer, 14 checker) прошли.
Все шесть adapter, пять kernel, 19 tool и 15 root gates прошли: fmt/check,
strict Clippy/Rustdoc, feature matrix и release builds. Root — 895/14, minimal —
165/3, project-cli — 182/3, build-cli — 201/3. Rust 1.98.1; версии и зависимости
не менялись. Проверены 322 исходных native hashes из provenance.

| Корпус | Заданий | Use=true | Predefined=true | YAML/XML/control hashes |
| --- | ---: | ---: | ---: | ---: |
| C1 | 83 | 47 | 27 | 168 |
| C2 | 186 | 36 | 122 | 374 |
| C3 | 187 | 37 | 120 | 376 |

Read-only пример `project_jobs` сравнил все 456 моделей с независимым native
reader; полное равенство подтверждено. Проверенные bytes сохранены. PATH был
пустым, XML не материализовался, платформа не запускалась.

Шесть IDE-процессов RU/EN на собственных частичных копиях C1/C2/C3 прочитали все
задания и вернули 83/186/187 имён в автодополнении. Проверены current MethodName/
Predefined/Use, invalid flag, повреждённый YAML, editor authority при подготовке
семантических входов и close. Текущие hashes и UTF-16 ranges с non-BMP/CRLF
совпали; machine trace RU/EN одинаков. Bytes 87/190/191 оригинальных файлов и
их копий сохранены, временные проекты удалены. Semantic coverage остаётся false:
остальные payloads намеренно отсутствуют. Peak RSS — 74 140–268 480 KiB,
сценарий — 0,66–22,66 s. Это локальный замер без общего memory budget.

Полный saved C1 checker в RU/EN проверил все 1 975 модулей с прежним SelfAssign
профилем. Все 26 findings, settings hash, source quota и coverage сохранились.
Ушли ровно 83 отказа по заданиям: metadata issues сократились с 1 601 до 1 518,
остальные причины не изменились. JSON одинаков, bytes всех 7 829 файлов сохранены.
Exit 2 остаётся из-за неполноты моделей.

Workspace MCP недоступен; CLI fallback `bsl-analyzer-app` 0.3.0 проверил два
уникальных новых BSL-тела: ноль ParseError, два SelfAssign, ноль failed files.
Reference MCP подтвердил коллекцию ScheduledJobs для серверного контекста;
описание API отсутствовало, platform help degraded. Настоящий IDE-клиент и 1С
runtime не запускались. Normal graph прежний 206/214; release analyzer/checker —
50 229 856 / 46 303 704 bytes. [Машинный отчёт](analyzer-scheduled-jobs-results.json)
содержит обезличенные gates и результаты.

## Ограничения

Этот адаптер не исполняет расписание, не валидирует сигнатуру обработчика и не
доказывает возможность запуска задания. Полная семантика, прочие модели, общий
heap/RSS budget, остальные IDE edits и Explorer остаются в работе. Исходная квота
не является лимитом памяти процесса; EF-18 остаётся `IN_PROGRESS`.
