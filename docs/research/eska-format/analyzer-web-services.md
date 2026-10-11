# Текущие Web-сервисы

WebService подключён к общей сессии как `CanonicalMetadata::WebService`.
IDE и checker используют один адаптер текущего YAML; Namespace, операции,
ProcedureName и имена параметров публикуются одним каноническим значением.
Invalid/unavailable очищают прежнюю модель. Это продолжение
[HTTP-сервисов](analyzer-http-services.md) в EF-18, без новых crates,
dependencies, команд или XML-экспорта.

## Граница модели

Адаптер сохраняет поля закреплённой
[native model](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/web_service.rs):
имя, Namespace, упорядоченные операции и логический URI модуля.
Операция содержит имя, ProcedureName и параметры. Native parameter хранит только
имя: типы и направление параметров, значения XDTO и прочие поля остаются в общей
модели проекта. Параметры с одинаковым именем в разных операциях сохраняют своего
родителя. UUID проверяются в исходной идентичности, но не входят в native модель.

Отсутствующие scalars становятся пустыми строками, как в
[XML-reader](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/xml_parser/web_service.rs).
Пустой ProcedureName допустим для модели. Дубли свойств и record вместо scalar
дают typed issue с диапазоном конкретного Namespace или ProcedureName.
Foreign namespace не подменяет одноимённое свойство схемы. Неполный descriptor
или параметр без captured properties не получают готовую таблицу операций.

Используются существующие native builders; модель и XML-parser не менялись.
Resolver учитывает source revision, вид и имя. Недоступный, несовместимый или
ошибочный зарегистрированный slot не читает XML и не возвращает прежнюю модель.
Для неканонического входа сохранён существующий XML-reader.

Name/file lookup использует один tracked resolver. HTTP/Web listings связывают
только объявленные BSL bodies через общий lookup в manifest; чтение YAML не
активирует модуль. Binary bodies не получают BSL backlink. Логический URI модели
соответствует native parser и сам по себе не доказывает наличие или читаемость
файла. Правка владельца не перевычисляет модель соседа и name/module index.

Имена доступны в completion `Метаданные.WebСервисы` / `Metadata.WebServices`.
Экспериментальный Rust enum расширен; exhaustive matches требуют нового варианта.
JSON schemas и протокольные команды не изменились.

## Проверки

56 unit и шесть файловых тестов адаптера, native 1 109 passed / четыре прежних
ignored, analysis 265 tests (121 unit/file, 130 analyzer, 14 checker) прошли.
Все шесть adapter, пять kernel, 19 tool и 15 root gates прошли: fmt/check,
strict Clippy/Rustdoc, feature matrix и release builds. Root — 895/14, minimal —
165/3, project-cli — 182/3, build-cli — 201/3. Rust 1.98.1; версии и зависимости
не менялись. Проверены 322 исходных native hashes из provenance.

| Корпус | Сервисов | Операций | Параметров | YAML/XML/control hashes |
| --- | ---: | ---: | ---: | ---: |
| C1 | 10 | 87 | 216 | 22 |
| C2 | 19 | 243 | 602 | 40 |
| C3 | 20 | 244 | 603 | 42 |

Read-only пример `project_web_services` сравнил все 49 моделей с независимым
native reader: полное равенство, включая логический URI, подтверждено.
Проверенные bytes сохранены. PATH пустой, XML не материализовался, платформа
не запускалась. Unit общей сессии проверил name/file lookup при pending теле
модуля, current Namespace/ProcedureName и переименование параметра, invalid/
unavailable, editor authority при semantic prepare и close. Существующие HTTP
тесты сохранились после объединения lookup объявленных bodies.

Шесть IDE-процессов RU/EN на собственных частичных копиях C1/C2/C3 прочитали все
сервисы и вернули 10/19/20 имён в completion. Проверены current Namespace/
ProcedureName, неверный ProcedureName, повреждённый YAML, semantic prepare и close.
Текущие hashes и UTF-16 ranges с non-BMP/CRLF совпали; machine trace RU/EN одинаков.
Bytes 14/23/24 оригинальных файлов и их копий сохранены, временные проекты удалены.
Semantic coverage остаётся false: прочие payloads намеренно отсутствуют.
Peak RSS — 74 324–272 512 KiB, сценарий — 0,54–21,61 s. Это локальный замер без
общего memory budget.

Полный saved C1 checker в RU/EN проверил все 1 975 модулей с прежним SelfAssign
профилем. Все 26 findings, settings hash, source quota и coverage сохранились.
Ушли ровно десять Web-service issues: metadata issues сократились с 1 508 до
1 498, остальные причины не изменились. JSON одинаков; bytes всех 7 829 исходных
файлов сохранены. Exit 2 остаётся из-за неполноты моделей.

Workspace MCP недоступен; CLI fallback `bsl-analyzer-app` 0.3.0 проверил четыре
уникальных BSL-тела, включая новый Web module fixture: ноль ParseError, четыре
SelfAssign, ноль failed files. Reference MCP подтвердил WebServices в серверном
контексте; описание API отсутствовало, platform help degraded. Настоящий IDE-клиент
и 1С runtime не запускались. Normal graph прежний 206/214; release analyzer/checker —
50 268 024 / 46 341 264 bytes. [Машинный отчёт](analyzer-web-services-results.json)
содержит обезличенные результаты.

## Ограничения

SOAP endpoint, WSDL/XDTO и сигнатуры обработчиков этим адаптером не проверены.
Прочие модели, полная семантика, общий heap/RSS budget, остальные IDE edits и
Explorer остаются в работе. Исходная квота не является лимитом памяти процесса;
EF-18 остаётся `IN_PROGRESS`.
