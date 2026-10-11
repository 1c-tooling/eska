# Текущие HTTP-сервисы

HTTPService подключён к общей сессии как `CanonicalMetadata::HTTPService`.
IDE и checker используют один адаптер текущего YAML; RootURL, шаблоны URL,
HTTPMethod и Handler публикуются одним каноническим значением. Invalid/unavailable
очищают прежнюю модель. Это продолжение [подсистем](analyzer-subsystems.md)
в EF-18, без новых crates, dependencies, команд или XML-экспорта.

## Граница модели

Адаптер сохраняет поля закреплённой
[native model](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/http_service.rs):
имя, RootURL, упорядоченные шаблоны URL с методами и логический URI модуля.
Шаблон содержит имя, Template и методы; метод — имя, HTTPMethod и Handler.
Повторяющиеся имена методов в разных шаблонах сохраняют своего родителя.
UUID проверяются в исходной идентичности, хотя native модель этих полей не хранит.

Отсутствующие scalars становятся пустыми строками, как в
[XML-reader](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/xml_parser/http_service.rs).
Пустой Handler допустим для модели. Дубли свойств и record вместо scalar дают
typed issue с диапазоном конкретного свойства, включая вложенный Handler.
Foreign namespace не подменяет одноимённое свойство схемы. Неполный descriptor
не получает готовую таблицу маршрутов. ReuseSessions, SessionMaxAge и прочие
поля остаются в общей модели проекта, но отсутствуют в native BSL-модели.

Используются существующие native builders; модель и XML-parser не менялись.
Resolver учитывает source revision, вид и имя. Недоступный, несовместимый или
ошибочный зарегистрированный slot не читает XML и не возвращает прежнюю модель.
Для неканонического входа сохранён существующий XML-reader.

Разрешение по имени и по файлу модуля использует один tracked resolver.
Listing связывает только объявленный `HTTPServices/<Name>/Ext/Module.bsl` с его
FileId; чтение YAML не активирует тело модуля. Binary bodies не получают BSL
backlink. Логический URI модели соответствует native parser и сам по себе
не доказывает наличие или читаемость файла. Правка владельца не перевычисляет
модель соседа и name/module index.

Имена доступны в completion `Метаданные.HTTPСервисы` / `Metadata.HTTPServices`.
Экспериментальный Rust enum расширен; exhaustive matches требуют нового варианта.
JSON schemas и протокольные команды не изменились.

## Проверки

52 unit и шесть файловых тестов адаптера, native 1 108 passed / четыре прежних
ignored, analysis 263 tests (120 unit/file, 129 analyzer, 14 checker) прошли.
Все шесть adapter, пять kernel, 19 tool и 15 root gates прошли: fmt/check,
strict Clippy/Rustdoc, feature matrix и release builds. Root — 895/14, minimal —
165/3, project-cli — 182/3, build-cli — 201/3. Rust 1.98.1; версии и зависимости
не менялись. Проверены 322 исходных native hashes из provenance.

| Корпус | Сервисов | Шаблонов URL | Методов | YAML/XML/control hashes |
| --- | ---: | ---: | ---: | ---: |
| C1 | 4 | 11 | 11 | 10 |
| C2 | 12 | 57 | 63 | 26 |
| C3 | 11 | 55 | 61 | 24 |

Read-only пример `project_http_services` сравнил все 27 моделей с независимым
native reader: полное равенство, включая логический URI, подтверждено.
Проверенные bytes сохранены. PATH пустой, XML не материализовался, платформа
не запускалась. Unit-тест общей сессии проверил name/file lookup ещё при pending
теле модуля, current RootURL/Template/HTTPMethod/Handler, invalid/unavailable,
editor authority при semantic prepare и close.

Шесть IDE-процессов RU/EN на собственных частичных копиях C1/C2/C3 прочитали все
сервисы и вернули 4/12/11 имён в completion. Проверены current поля, неверный
Handler, повреждённый YAML, semantic prepare и close. Текущие hashes и UTF-16
ranges с non-BMP/CRLF совпали; machine trace RU/EN одинаков. Bytes 8/16/15
оригинальных файлов и их копий сохранены, временные проекты удалены. Semantic
coverage остаётся false: прочие payloads намеренно отсутствуют. Peak RSS —
73 412–269 236 KiB, сценарий — 0,43–19,59 s. Это локальный замер без общего budget.

Полный saved C1 checker в RU/EN проверил все 1 975 модулей с прежним SelfAssign
профилем. Все 26 findings, settings hash, source quota и coverage сохранились.
Ушли ровно четыре HTTP-service issues: metadata issues сократились с 1 512 до
1 508, остальные причины не изменились. JSON одинаков; bytes всех 7 829 исходных
файлов сохранены. Exit 2 остаётся из-за неполноты моделей.

Workspace MCP недоступен; CLI fallback `bsl-analyzer-app` 0.3.0 проверил три
уникальных BSL-тела, включая новый HTTP module fixture: ноль ParseError, три
SelfAssign, ноль failed files. Reference MCP подтвердил HTTPServices в серверном
контексте; описание API отсутствовало, platform help degraded. Настоящий IDE-клиент
и 1С runtime не запускались. Normal graph прежний 206/214; release analyzer/checker —
50 264 112 / 46 331 456 bytes. [Машинный отчёт](analyzer-http-services-results.json)
содержит обезличенные результаты.

## Ограничения

Исполнение endpoint, сигнатуры обработчиков и полнота HTTP-протокола этим адаптером
не проверены. Прочие модели, полная семантика, общий heap/RSS budget, остальные
IDE edits и Explorer остаются в работе. Исходная квота не является лимитом памяти
процесса; EF-18 остаётся `IN_PROGRESS`.
