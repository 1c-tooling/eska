# Текущие подписки на события

`EventSubscription` подключён к общей сессии анализа через отдельный вариант
`CanonicalMetadata::EventSubscription`. Дерево метаданных, IDE и checker используют
один YAML-reader и адаптер. Изменения Source, Event и Handler в открытом документе
публикуют новую каноническую модель; invalid/unavailable очищают прежнее значение.
XML не материализуется, второй индекс или parser не добавлен. Это компонент EF-18.

## Проекция и инвалидация

Адаптер сохраняет UUID, имя, optional Comment, Event и Handler. Как в закреплённом
[native reader](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/xml_parser/event_subscription.rs),
Source остаётся строкой с альтернативами через `;`, включая порядок и дубли.
Общая проекция проверяет namespace ключей `Type`/`TypeSet` и QName значений:
префикс ссылки на current-config нормализуется в `cfg:`. Alias/default namespace
дают ту же модель. Неизвестные поля, чужие или неразрешённые QName, дубли свойств
и неверная структура дают typed issue с диапазоном текущего свойства.
Исходные preserved XML fragments и формат проекта не меняются.

В локальную [модель ядра](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/event_subscription.rs)
добавлены `Eq` и `from_parts`; нативный XML-parser сохранён. Resolver сначала
проверяет зарегистрированный slot и source revision. Pending/invalid, несовместимый
вариант и другое имя не читают XML и не возвращают старую модель. Чужая правка
не перевычисляет соседнюю подписку или name index. Без canonical slot прежний
XML-путь ядра сохраняется.

SDK добавляет подписки в устойчивый listing. `Метаданные.ПодпискиНаСобытия` /
`Metadata.EventSubscriptions` получает имена того же состава через существующее
автодополнение. Это не новая LSP-команда или схема JSON. Экспериментальный Rust enum
расширен новым вариантом; совместимость exhaustive matches прежнего enum не обещана.

## Проверки

40 unit и шесть файловых тестов адаптера, 1 105 native tests / четыре прежних ignored,
257 analysis tests (117 unit/file, 126 analyzer, 14 checker) прошли. Пройдены шесть
adapter gates, пять kernel gates, 19 tool gates и 15 root gates: fmt/check, strict
Clippy/Rustdoc, feature matrix и release builds. Root — 895/14, minimal — 165/3,
project-cli — 182/3, build-cli — 201/3. Использован Rust 1.98.1; зависимости и версии
не менялись. Проверены 322 исходных native SHA-256 из provenance.

| Корпус | Подписки | Полное равенство native model | YAML/XML/control hashes |
| --- | ---: | --- | ---: |
| C1 | 91 | Да | 184 |
| C2 | 462 | Да | 926 |
| C3 | 531 | Да | 1 064 |

Read-only пример `project_subscriptions` сравнил все 1 084 модели с независимым
нативным reader. UUID, Comment, Source, Event и Handler совпали; проверенные
исходные bytes сохранены. PATH был пустым, платформа и XML-экспорт не запускались.

Шесть IDE-процессов C1/C2/C3 в RU/EN прочитали все подписки в собственных частичных
копиях с полным составом. Автодополнение вернуло 91/462/531 имён. Подтверждены
Source/Event/Handler overlay, invalid QName, отказ повреждённого YAML, сохранение
editor authority при semantic preparation и восстановление после close. Текущие
hash/UTF-16 диапазоны с non-BMP/CRLF и machine trace RU/EN согласованы. Проверены
95/466/535 оригинальных файлов и их копии; временные проекты удалены. Peak RSS —
73 556–281 540 KiB, сценарий — 0,46–15,61 s. Это локальный замер частичного состава;
semantic_profile_complete и semantic_complete остаются false.

Полный saved C1 checker проверил 1 975 модулей на обоих языках с прежним профилем
SelfAssign. Все 26 findings, settings hash и source quota совпали с предыдущей
приёмкой. Число metadata issues сократилось с 1 692 до 1 601: ушли ровно 91 отказ
по подпискам, остальные причины и coverage не изменились. JSON RU/EN совпал,
все 7 829 файлов исходного проекта сохранены. Exit 2 остаётся из-за неполных моделей.

Workspace MCP недоступен: CLI fallback `bsl-analyzer-app` 0.3.0 проверил три новых
BSL-тела, ноль ParseError и три SelfAssign. Reference MCP подтвердил коллекцию
EventSubscriptions в серверном контексте; описание API отсутствовало, platform
help работал в degraded режиме. Для реальных probes выбран серверный общий модуль;
доступность платформенного API зависит от контекста выполнения.

Normal graph прежний — 206 package names / 214 versions; release analyzer/checker —
50 231 040 / 46 302 128 bytes. [Машинный отчёт](analyzer-event-subscriptions-results.json)
содержит обезличенные gates и результаты.

## Ограничения

Source остаётся непрозрачной строкой модели ядра: корректность всех целевых объектов,
доступность события и сигнатура Handler этим адаптером не доказаны. Alias QName
проверены unit tests; реальные корпусы используют `cfg:`. Полная семантика, прочие
модели, общий heap/RSS budget, остальные IDE edits и Explorer остаются в работе.
Установленная 1С обнаружена, но CF и клиент для этой части не запускались;
настоящий VS Code client не проверен. EF-18 остаётся `IN_PROGRESS`.
