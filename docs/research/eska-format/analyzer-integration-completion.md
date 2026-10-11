# Автодополнение сервисов интеграции

Имена объявленных сервисов доступны после `Метаданные.СервисыИнтеграции.` и
`Metadata.IntegrationServices.`. Это закрывает пробел, обнаруженный при
[подключении current channel models](analyzer-integration-services.md).
Протокол и состав поддержанных канонических значений не менялись; EF-18 остаётся
`IN_PROGRESS`.

## Граница исправления

Контекст принадлежит native
[mdo completion](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/ide/src/completion/mdo_completion.rs).
Распознавание интеграционной коллекции добавлено к существующему пути, который
проверяет semantic root claim и availability mask корня/свойства. Параметр или
обычная переменная с именем Metadata не подменяет глобальный объект. В строках
и комментариях проектные сервисы не предлагаются. RU/EN spelling и смешанный
регистр поддерживаются; detail SDK локализуется native locale. Существующий
stdio DTO передаёт имя, вид и текст вставки, но не detail; его схема сохранена.

Новый getter `RootDatabase::integration_service_names` следует прежним правилам
видимости main/extension и дедупликации. При bootstrapped substrate он читает
только listing; current YAML каналов и BSL bodies целей не активируются. Прежний
legacy merged-config путь сохранён для неканонических входов. Invalid канал не
удаляет объявленное имя сервиса и не восстанавливает прежнюю модель handlers.
Ядро отвечает за язык и context, общая сессия — за structural identities и current
canonical inputs. Второго поиска имён в transport или расширении редактора нет.

Для этого уже используемая библиотека `ide` 0.2.85 включена в локальный kernel
workspace из той же ревизии `9021ba0`: 173 исходных файла, 2 616 570 bytes. Версии
пакетов SDK не изменились; kernel lock получил уже используемые SDK зависимости
этой библиотеки. Source provenance теперь содержит 495 исходных hashes.
Исправлены семь прежних Rustdoc links в четырёх файлах этой копии. Лицензии,
исходный reference checkout и publication pipeline не менялись.
Пробелы в двух исходных тестовых строках сохранены; локальный `.gitattributes`
исключает их намеренный whitespace из проверки diff.
Расширен экспериментальный Rust trait; внешние implementors должны добавить
getter. JSON schema и protocol handshake не изменились.

## Проверки

Два новых SDK-теста проверяют три spellings в RU/EN, localized detail, pending
каналы, параметр, обычную переменную, комментарий и строку. Process test RU/EN
проверяет имена в готовом и invalid current YAML. Все 270 SDK tests прошли
(125 unit/file, 131 analyzer, 14 checker), включая прежние HTTP/Web completions.
Native workspace — 2 626 passed / 20 прежних ignored вместе с Rustdoc examples.
Strict Clippy/Rustdoc, fmt/check и feature matrix проверены у обоих workspaces;
root gates и feature matrix также прошли.

Реальные stdio-сценарии, сохранность исходников, сравнение C1 checker и текущие
release/graph замеры записаны в
[машинный отчёт](analyzer-integration-completion-results.json).
Проверки запускаются на собственных частичных копиях C1/C2/C3 с прежней полной
композицией и пустым PATH. Это локальный IDE-host probe, а не запуск настоящего
редактора или 1С runtime. Native completion не означает готовность полного
inference, навигации к этим ссылкам или проверки сигнатур handlers.

Workspace MCP недоступен; установленный CLI 0.3.0 проверил десять уникальных BSL
фрагментов с реальными телами и новыми completion fixtures: ноль ParseError,
один SelfAssign, ноль failed files. Прочие модели, полная семантика, общий memory
budget, оставшиеся IDE edits и Explorer ещё в работе.
