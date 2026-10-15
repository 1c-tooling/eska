# Диагностика запросов по текущим метаданным в Explorer

Explorer commit `709ff29` подключает отдельный `queryAnalysis` opt-in к существующим
host, writer, journal и language provider. Проверенный producer соответствует
[текущим form/DCS inputs](query-context-read-view.md); более поздние library output
и consumer checks этим VSIX не публикуются. Версии инструментов сохранены.

Semantic `.sdbl` использует `eska-query-analysis` при точной capability. Inventory
правил приходит из native analyzer. Старый неизвестный peer сохраняет прежний
syntax route; known-disabled capability или отказ current metadata не превращается
в успешный syntax результат. Перед публикацией проверяются source bytes/hash,
wire version, snapshot, root, settings/composition hashes, включённые rules и
соответствие UTF-8 offsets диапазонам UTF-16. Producer messages сохраняются.

Metadata pending/change/close/watch сразу отзывает dependent findings, включая
случай неизменного текста query. Watcher ждёт ACK в том же input slot; declined
batch удерживает недоступность до соответствующего accepted recovery. Повторное
соединение отзывает предыдущий epoch синхронно, queued watcher не теряется при
negotiation. Новый writer, revision clock или saved fallback не создаются.

2026-10-15 TypeScript и полный Node-набор прошли: **310/310, без пропусков**.
Настоящий framed transport/provider проверен на CF/CFE × RU/EN, current metadata,
зависимую видимость, stale/late replies, cancellation/deadline/quota, watcher,
restart/import и отключённый QueryParseError. Изменение состава rules не требует
нового клиентского списка.

Свежий VSIX 0.0.11 проверен в отдельном installed VSCodium 1.109.51242 на Linux:
generated CF/configured CFE и CF из выгрузки 1С. Прошли dirty metadata/query,
native diagnostics и BOM/CRLF/Unicode ranges, invalid-current refusal,
restart/import replay и readonly protected view с отзывом прежних findings.
Standalone native CFE без доказанной базы вернула `analysis_scope_unsupported`;
после отдельного выбора syntax доступен синтаксис. Эта CFE не засчитана как
положительная semantic composition.

Source bytes сохранены, все 12 analyzer children завершены. VSIX содержит
1314 entries; неизменность artifact pair и соответствие 52 compiled payload files
проверены. Helpers исключены из поставки. Предварительное ожидание неверного
кода initialize отказа исправлено по существующему SDK contract; production
после полного Node gate и упаковки не менялся.

[Обезличенные результаты](explorer-query-analysis-results.json) отделяют Node,
framed и установленную проверку. Использованы native editor API и commands,
без физических нажатий и без Save. Проверенный producer передавал общий
code/severity fallback; исправление локализации SDK принято отдельно и не
приписано прежней паре. Parameters/consumer/semantic coverage остаётся false;
editable opening, guarded Save, остальные группы/платформы и EF-16 ещё нужны.

Последующий `d094496` исправляет byte→UTF-16 проверку внутренней CRLF границы и
одиночного CR по native TextCoordinates. Полный Node gate — **311/311, без skipped**;
установленная матрица выше относится к `709ff29`, новый VSIX ей не подменён.
