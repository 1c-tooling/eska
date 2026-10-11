# Текущие сервисы интеграции

IntegrationService подключён к общей сессии как
`CanonicalMetadata::IntegrationService`. IDE и checker получают каналы и
ReceiveMessageProcessing из текущего YAML одним адаптером. Invalid/unavailable
очищают прежние bindings. Это продолжение [Web-сервисов](analyzer-web-services.md)
в EF-18, без новых crates, dependencies, команд или XML-экспорта.

## Граница модели

Адаптер сохраняет поля закреплённой
[native model](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/integration_service.rs):
имя сервиса, порядок каналов, имя и ReceiveMessageProcessing каждого канала.
Отсутствующий handler становится пустой строкой, как в
[XML-reader](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/xml_parser/integration_service.rs).
Пустые bindings сохраняются в модели канала, но не входят в `receive_handlers()`.
Направление сообщений, настройки транспорта и прочие свойства остаются в общей
модели проекта. UUID проверяются в исходной идентичности, но не входят в native
модель. Используются существующие builders; модель и XML-parser не менялись.

Record вместо scalar и дубли ReceiveMessageProcessing дают typed issue с
координатами конкретного свойства. Foreign namespace не подменяет binding.
Неполный descriptor и канал без captured properties не получают готовую таблицу.
Resolver учитывает source revision, вид и имя. Ошибочный зарегистрированный slot
не возвращает прежний handler и не читает XML. Для неканонического входа сохранён
существующий XML-reader.

Name/file lookup использует один tracked resolver. Модуль связывается через общий
HTTP/Web/integration lookup объявленных BSL identities из manifest; отсутствующий
или binary body остаётся без backlink. Загрузка YAML не читает BSL. Правка канала
не перевычисляет соседний сервис и name/module index. Экспериментальный Rust enum
расширен; exhaustive matches требуют нового варианта. JSON schemas не менялись.

## Проверки

60 unit и шесть файловых тестов адаптера, native 1 110 passed / четыре прежних
ignored, analysis 268 tests (123 unit/file, 131 analyzer, 14 checker) прошли.
Все шесть adapter, пять kernel, 19 tool и 15 root gates прошли: fmt/check,
strict Clippy/Rustdoc, feature matrix и release builds. Root — 895/14, minimal —
165/3, project-cli — 182/3, build-cli — 201/3. Версии и зависимости не менялись;
проверены 322 исходных native hashes из provenance.

Во всех C1/C2/C3 найдено по одному сервису, четыре канала и два непустых bindings:
три полные модели, 12 каналов и шесть handlers совпали с независимым native reader.
Read-only пример `project_integration_services` сохранил bytes четырёх контрольных
YAML/XML/control файлов каждого корпуса. PATH пустой, XML не материализовался.

Unit общей сессии проверил name/file lookup при pending BSL body, current имя
канала и binding, invalid/unavailable, editor authority при semantic prepare и
close. Native UnusedLocalMethod действительно использует binding: после смены
SavedReceive на CurrentReceive прежняя процедура становится неиспользуемой,
новая перестаёт ею быть, BSL bytes не меняются. Saved batch и resident session
совпали по этому finding, source hash, диапазону и сообщению в RU/EN.

Шесть IDE-процессов RU/EN на собственных частичных копиях C1/C2/C3 проверили current
ReceiveMessageProcessing, invalid property, повреждённый YAML, semantic prepare
и close. Current hashes и UTF-16 ranges с non-BMP/CRLF совпали; machine traces RU/EN
одинаковы. Bytes пяти оригинальных файлов и их копий каждого корпуса сохранены,
временные проекты удалены. Semantic coverage остаётся false: прочие payloads
намеренно отсутствуют. Измерения в [машинном отчёте](analyzer-integration-services-results.json)
относятся к отдельным локальным сценариям, а не общему memory budget.

Полный saved C1 checker проверил все 1 975 модулей с прежним SelfAssign профилем.
26 findings, settings hash, source quota и coverage сохранились. Ушёл ровно один
integration-service issue: metadata issues сократились с 1 498 до 1 497, остальные
причины не изменились. JSON RU/EN одинаков; bytes всех 7 829 исходных файлов
сохранены. Exit 2 остаётся из-за неполноты моделей.

Workspace MCP недоступен; CLI fallback `bsl-analyzer-app` 0.3.0 проверил все три
реальных тела сервиса и новый fixture: три уникальных BSL-тела, ноль ParseError,
один SelfAssign, ноль failed files. Reference MCP подтвердил IntegrationServices
в серверном контексте с 8.3.17; описание API отсутствовало. Платформа и настоящий
IDE-клиент не запускались. Normal graph прежний 206/214; release analyzer/checker —
50 296 008 / 46 363 176 bytes.

## Ограничения

На момент подключения модели закреплённое ядро не предлагало имена сервисов в completion
`Метаданные.СервисыИнтеграции` / `Metadata.IntegrationServices`: пять возвращённых
общих кандидатов не являются сервисами из проекта. Подключение модели обработчиков
не исправляло эту отдельную границу языкового ядра. Теперь она закрыта
[локальным исправлением native completion](analyzer-integration-completion.md).
Сигнатуры handlers, обмен
сообщениями, направления и транспорт runtime здесь не проверены.
Прочие модели, полная семантика, общий heap/RSS budget, остальные IDE edits и
Explorer остаются в работе. EF-18 — `IN_PROGRESS`.
