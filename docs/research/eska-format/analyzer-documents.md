# Черновики YAML, прав роли и BSL в отдельном сервере

`eska-analyzer` объявляет `textDocumentSync` с `openClose=true`, `change=1`
(full text) и `save.includeText=false`. Экспериментальный ESKA protocol `0.1`
сохраняется; `documentSync=true`. [Права роли](analyzer-roles.md) подключены
через `roleRightsSync=true`. Теперь отдельно подключена
[pull-диагностика syntax](analyzer-diagnostics.md) и [отмена запросов](analyzer-cancellation.md);
[Completion](analyzer-completion.md), [definition](analyzer-definitions.md),
[property plans](analyzer-property-plans.md) и [saved file events](analyzer-watched-files.md)
теперь используют ту же сессию. Проверки ниже относятся к первому компоненту
document sync. Старый IDE protocol приложения не изменён.

[Declared `.sdbl`](analyzer-query-syntax.md) используют тот же lifecycle после
явного `initializationOptions.eska.querySyntax=true`; didOpen передаёт languageId
`sdbl`. Без negotiation query input отклоняется до изменения host revision.
Query syntax не включает metadata/consumer semantics.

Транспорт хранит открытые пути и согласованный querySyntax. Текст, версии, origin, hash,
текущие модели и source-byte квота находятся в общем `AnalysisSession`;
отдельной копии редакторских моделей в сервере нет.

## Приём документов

Поддерживаются стандартные уведомления после initialize/initialized:

| Уведомление | Поведение |
|---|---|
| `textDocument/didOpen` | Один declared YAML/rights/BSL становится editor input; повторное открытие отклоняется |
| `textDocument/didChange` | Ровно одна полная замена, без range/rangeLength; версия должна вырасти |
| `textDocument/didSave` | Подтверждает текущее состояние без смены версии/ревизии и чтения диска |
| `textDocument/didClose` | Убирает редакторскую authority и читает актуальный saved файл, включая отказ чтения |

URI должен быть local `file:` внутри канонического source root выбранного
проекта, без query/fragment. Percent encoding декодируется библиотекой `url`,
включая Unicode и пробелы. Только объявленные владельцы YAML, права ролей и BSL-модули
являются входами; `languageId` непустой, но не переопределяет классификацию.
Необъявленные ресурсы, binary modules и внешние файлы отклоняются. Сам файл
может отсутствовать: open использует переданные редактором байты, не disk read.

Версия документа — signed 32-bit integer LSP; версия сессии — прежняя непрозрачная
десятичная строка. YAML, права и BSL меняют один `snapshot`. Accepted update, включая
невалидный YAML или quota refusal, повышает ревизию и очищает прежнюю модель/текст.
Отклонённая версия, URI или lifecycle не меняют текущие входы.

Сохранение следует [editor ownership LSP](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/textDocument/didOpen.md):
пока файл открыт, байты клиента являются authority. Даже если клиент прислал
необязательный `didSave.text`, он не заменяет последнюю full-document версию.
Закрытие читает диск заново. Удалённый или повреждённый saved файл возвращает
unavailable, а не предыдущие свойства. `eska/metadata/read` видит текущий YAML;
`eska/metadata/refresh` открытого владельца возвращает `overlay_active`.

## События состояния

На каждое поддержанное уведомление сервер отправляет отдельное JSON-RPC
notification, без `id`. Это не response на клиентское уведомление.
Имена объявлены в `experimental.eska.inputNotifications`.

`eska/inputChanged` содержит `snapshot`, `source`, `input` и `resources`.
`input.kind` различает metadata/role_rights/form_dialog/predefined_data/module; state сохраняет origin/document version,
source hash, readiness и coded failure. Для metadata отдельно видны adapter
readiness и неполнота. Текст BSL в событие не копируется, свойства YAML клиент
получает metadata request. Для `didSave` snapshot остаётся прежним.

`eska/inputRejected` содержит текущий `snapshot`, `operation` и структурированный
`error` с `data.kind`. Например, `stale_document_version`, `document_not_open`,
`document_already_open`, `full_document_change_required`, `document_uri_invalid`,
`document_outside_source`, `unknown_source`, `binary_module`. Current input
после такого события не считается очищенным или успешно пустым.

После события клиент использует новый token; запрос со старой ревизией получает
`stale_revision`. При reconnect старый session UUID по-прежнему отклоняется.
Состав manifest/configuration не переоткрывается уведомлениями документов;
для structural changes пока нужен restart.

## Ограничения и проверки

Показанные ниже результаты относятся к `edab5d7`, до подключения pull-диагностики.
Текущие gates и размер бинарника — в [результатах диагностики](analyzer-diagnostics.md).

Входной frame сервера увеличен с 1 до 4 MiB; header 8 KiB и ответ 64 MiB
сохранены. Существующий IDE reader приложения сохраняет прежний предел 1 MiB.
Предел 4 MiB относится ко всему JSON frame с URI/экранированием, а не только
к размеру текста. Oversized framing завершает процесс. Per-document bound
и общая source-byte квота остаются отдельными; они не ограничивают heap/RSS.

Прямой optional dependency `url` включён только `analyzer-cli`; версия 2.5.8
уже присутствовала транзитивно, число packages не выросло. Граф — 206 имён,
214 разрешённых версий; release binary — 44604008 bytes. Git, платформа,
HTTP-клиент, debugger и LSP runtime не подключены.

24 unit/file, 14 analyzer process и 9 checker process tests прошли. Новый набор
проверяет текущие свойства, exact BSL BOM/CRLF/hash, saved/editor/close,
stale version/revision, invalid/recovery, quota refusal, URI и сбалансированный
lifecycle, full-sync rejection, frame более 1 MiB и равенство RU/EN.
Основное приложение: 887 passed / 14 прежних ignored; minimal — 161/3,
project-cli — 177/3, build-cli — 196/3. fmt/check/strict Clippy/test/Rustdoc
прошли для приложения и отдельно для всех feature profiles инструмента.

На C1/C2/C3 read-only процесс проверил один YAML и связанный BSL, refresh
активного редактора, stale update, save и восстановление saved input.
Финальная общая ревизия — `5`, ответы EN/RU совпали кроме свежего UUID.

| Corpus | Peak RSS EN/RU, KiB | Текущие draft source bytes |
|---|---:|---:|
| C1 | 37848 / 37240 | 8059 |
| C2 | 171852 / 172004 | 18101 |
| C3 | 195376 / 195412 | 18101 |

Хешированы четыре файла каждого corpus: manifest, configuration, выбранный YAML
и BSL; все сохранились. Остальные файлы в этом замере не хешировались.
`PATH` пустой, временные настройки собственных C2/C3 удалены. Exit 0 здесь
подтверждает lifecycle, не полную диагностику или сборку 1С. Никакие документы
не записывались сервером и реальные BSL-семантика/языковые запросы не выполнялись.

[Машинный отчёт](analyzer-documents-results.json) содержит gates и измерения.
EF-18 остаётся `IN_PROGRESS`: остальные language operations, полное покрытие
канонических моделей, global memory budget и защищённые edit plans ещё не готовы.
