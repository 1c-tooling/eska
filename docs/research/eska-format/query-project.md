# Проект с отдельными запросами — EF-23, часть 3

Opt-in core API `import_designer_queries` создаёт [обратную карту версии 5](source-manifest-v5.schema.json).
`export_designer` и `SourceReader` читают её; прежние import defaults не меняются.
Descriptor schema 2, form body schema 2, conversion rules 1 и протокол
[query references](query-references.md) сохранены. Нативный профиль — обычная
configuration/Designer 2.20, managed forms logform 2.20.

## Единственный текст и привязка

`source.json.query_resources` объявляет native resource `path`, metadata
`descriptor` и полный набор относительных plain `sources`. Native scoped slot
живёт только в source XML-ссылке, не дублируется в карте. Весь набор ссылок
обязан совпасть с объявлением. Метаданные формы/шаблона определяют владельца;
путь `.sdbl` не определяет native slot и не даёт права на редактирование.

СКД извлекается только из объявленного Template/CommonTemplate с действительным
native `TemplateType = DataCompositionSchema`. Даже схема без запросов имеет
запись с пустым набором sources: позднее нельзя добавить скрытый inline query
или исключить ресурс из обработки. Другие XML и бинарные payloads сохраняются
как opaque ресурсы.

Динамический список хранит ссылку в единственном body `form.yaml`, в native
Attributes stanza. Второй `Form.xml` не создаётся. Схема СКД хранит ссылки
в своём `Template.xml`. Оба ресурса полностью избавляются от source markers
при экспорте; остальные части XML сохраняются.

Первый импорт создаёт:

- `Form/Queries/Attribute_<native ID>.sdbl` для списка;
- `Template/Queries/Query_<dataset>.sdbl` для корневого набора СКД;
- `Template/Queries/Query_<union>/Query_<dataset>.sdbl` для вложенного набора.

В имени сохраняются Unicode letters/digits и `_`; прочие UTF-8 bytes percent
encoded. Префикс исключает Windows device names. Коллизии, case aliases,
файл на месте каталога, traversal и чужой owner prefix дают отказ. При изменении
имени набора уже записанная ссылка сохраняет путь; actual binding читается
из нового native scope. Это не операция безопасного переименования проекта.

`.sdbl` — UTF-8 plain text без служебных директив. Сохраняются decoded characters,
пакет, комментарии и логические переводы строк; query text не форматируется.
Original byte fingerprints в карте по-прежнему описывают импортированный native
input, а не утверждают, что изменённый текст совпадает с ним.

## Чтение, экспорт и отказы

`query_paths` перечисляет declarations без чтения соседних ресурсов. `declared_query`
возвращает owner, native descriptor/resource и объявленный metadata UUID.
`read_query_text` читает только выбранный plain файл. `read_query_resource` проверяет
действительный reference document и полный набор путей, не читая plain values.
Для формы он получает XML из единственного YAML; его XML ranges относятся
к сгенерированному тексту, не к физическим YAML coordinates. Оба входа СКД
(owner и payload) и итоговый XML ограничены заданным byte limit.

`read_form_dialog_text` материализует native dialog, читая только значения
данной формы. Открытие проекта и адресное чтение допустимы при пропавшем соседнем
plain файле; полный экспорт обязательно отказывает. Whole export проверяет
весь физический набор, роли/типы/profile ресурсов, source declarations и XML
characters до публикации. Missing/extra файлы, смешанный inline/reference
query, чужой profile и неполная карта не подменяются старым inline text.
При отказе прежний output сохраняется.
Как и в прежнем acceptance API, существующий destination допустим только
для побайтно одинакового повторного экспорта. Изменённый вариант требует
нового destination; protected writer публикует отдельные immutable generations.

Старые layout targets не понижают map 5: подготовка forms/readable layout
возвращает byte no-op. Новая явная query migration ещё нужна.
В части 3 `accept_source` сохранял полный map 5 snapshot, а `prepare_protected`
временно отказывал с `source_query_protection_unavailable`.
[Часть 4](query-protected.md) подключает supplier selection и source map 4;
объявленная identity по-прежнему не означает accepted permission. Точечный
query editor и явная migration ещё нужны.

[C1 результаты](query-project-results.json) фиксируют independent old-layout
control: no-op совпал по 8 769 файлам и 8 064 каталогам; все 823 адресных form
reads совпали с control. Map содержит 153 resources (51 СКД и 102 формы),
150 plain файлов (27 запросов СКД и 123 списка). Правка одного plain файла
изменила один native ресурс; правки всех 150 изменили только 126 queried
resources. Scoped values повторно разобраны, source изменения ограничены
`.sdbl`, input прежний. Повторный no-op export стабилен.

Восемь owned fixture tests проверяют import/export/repeat, one-file edit,
addressed reads при пропавшем соседнем файле, missing/extra/inline/profile
отказы с сохранением прежнего output, declaration/path collisions, naming и
rename, пустые СКД с обязательной записью, no-downgrade старых targets и явный
временный отказ непринятой protection (позднее заменён protected no-op/control).
Прошли девять Cargo/SDK gates: 685 unit, 570
integration и 359 SDK tests; 24 прежних opt-in tests ignored. JSON Schema и
локальные ссылки проверены. CF/build/execution изменённых запросов и семантика
языка этой частью не принимаются. EF-23 остаётся
`IN_PROGRESS`; анализ языка и зависимостей относится к EF-27.
