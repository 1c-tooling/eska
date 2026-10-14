# Редактирование после изменения состава при импорте

EF-25 связывает editor authority с текущей проверенной картой исходников.
Раньше protected build проверял rename/add, но editor API продолжал искать пути
и UUID только в первоначальном снимке. Теперь переименованный модуль и новый
owner доступны под прежним независимо переданным support pin.

## Привязки и поддержка

`AcceptedBase::bind_source` и cancellable вариант проверяют current root/map,
root identity и допустимость изменённой карты через тот же validator,
что protected prepare.
Новая композиция не становится принятой базой: storage, pin, исходные stencils,
supplier rules и inherited freeze остаются прежними. Новые owners получают
только authority, уже разрешённую проверкой полного объявления и родителя.
Повторное связывание derived handle сначала открывает исходный store.

Если source map не изменена, заново читаются карта и authoring descriptors,
без полного обхода модулей и opaque payloads. Изменённая карта требует полного
снимка для существующей проверки состава. Aliases, root UUID substitution,
frozen rename, неподтверждённые identities новой карты и cancellation не дают частичной
authority. Эти Rust API экспериментальные; storage/source-map schemas прежние.

`EskaSource::edit_authority` добавляет project import guard. SDK заново открывает
текущий source и bindings перед каждой property/BSL preview. Process test после
rename/add запускает новый сервер, сохраняет original pin и unsaved YAML
annotation, возвращает versioned WorkspaceEdit на RU/EN без записи на диск.
Этот тест не подтверждает refresh уже работающего сервера.

JSON preview `eska-build import` дополнен массивом `support`: для изменённых UUID
он содержит `state`, `native_reason`, `reason`, `allowed` и `baseline` из общего
Permission API. Решение относится к Resource; оно не разрешает любую операцию
над объектом. У unresolved новых UUID состояние Unknown. Ключи и значения
одинаковы на RU/EN; существующий human report перечисляет files/conflicts.

## Сохранённый план

Saved module/property/form/query plan сохраняет fingerprint карты и всех
authoring descriptors, а project consumer — также путь к import guard.
Публикация проверяет эту границу до общего atomic writer. Другой root,
изменившиеся descriptors/map или незавершённый import отказывают до записи;
новый bound handle caller не снимает fence старого плана. Проверка оригинальных
байтов редактируемого файла тоже сохраняется.

Fence консервативен: изменение даже comment/scalar другого сохранённого YAML
descriptor требует нового плана. Правка несвязанного BSL/query сама по себе
не меняет fingerprint состава. Guard проверяется при открытии authority и
публикации saved plan; это не протокол координации импорта с unsaved вкладками.

## Проверки

Пять core cases проверяют rename, add, old pin, exact BOM/CRLF module bytes,
stale fence, invalid/cancelled binding и новую managed form с plain query.
Отдельный project case проверяет guard для authority и уже подготовленного
saved edit. SDK process case проверяет current property targets и unsaved
annotations на обеих локалях; CLI process matrix проверяет новый JSON support.

Все 22 Rust gates прошли: root 814 unit/600 integration, all-features также
27 build/52 project process cases; SDK 185/159/19, adapter 70 + 12,
minimal/build-core/build-CLI и Rustdoc. 24 opt-in root cases пропущены.

Четыре копии проектов после native импорта прошли saved property/module edits
через source-only consumer, отказ по live guard, build и независимый dump на
1С 8.3.27.2325. Состав 17/111/14/8 файлов сохранился. XML равен прежнему dump
с одной ожидаемой Comment правкой; BSL — с одним добавленным comment и
разделяющими переводами строк. Служебный CDF configVersion исключён из сравнения;
UUID/order, остальные XML values и opaque bytes сохранены. Source bytes,
old pin и точные settings проверены отдельно.
[Обезличенные результаты](post-import-edit-bindings-results.json).

Четыре изменённых модуля проверены CLI fallback bsl-analyzer-app 0.3.2:
complete, failed files 0, SyntaxError 0; прочие style/context findings 0/1/1/2.
Workspace MCP недоступен, bsl-analyzer.toml в собственных demos отсутствует;
использованы defaults. Новые runtime/client сценарии и production проекты
этой частью не принимаются; проверена Linux.

Согласованный refresh/restart IDE, защита unsaved вкладок во время замены
состава, общий new/init/import и приёмка EF-25/EF-16 ещё нужны. MVP не закрыт.
