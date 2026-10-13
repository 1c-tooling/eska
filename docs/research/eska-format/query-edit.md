# Точечное редактирование запросов — EF-23, часть 5

`prepare_query_edit(root, base, path, updated)` возвращает общий `SourceEditPlan`
для одного объявленного `.sdbl`. `prepare_query_edit_text(base, path, original,
updated)` возвращает `QueryEditPreview` для несохранённого текста редактора.
Preview содержит accepted path/pin, exact updated text и признак byte no-op;
он не может публиковать файл. Caller проверяет собственную ревизию документа
и независимо выбранный pin. Новых CLI команд или IDE protocol messages нет.

Владельца определяет только принятая `QueryResource` и native metadata UUID
её descriptor. Имена файлов, рабочие `source.json`, YAML UUID или references
не предоставляют права. Прежняя операция `Resource` учитывает native support
и freeze владельца/metadata предков. Undeclared path, alias по регистру и путь
ресурса вместо plain файла отказывают. Protected writer использует тот же
внутренний accepted lookup; дополнительной публичной модели разрешений нет.

Вся переданная строка становится единственным значением запроса. Сохраняются
комментарии, Unicode, кавычки, пакет, пробелы и окончания строк; импортёр или
editor не форматируют её. Пустой запрос допускается как сохранённое значение.
Новый текст проверяется на XML 1.0 characters и hard limit 64 MiB. Invalid
current characters можно исправить valid replacement. Это проверка хранения,
не синтаксиса языка или потребителей СКД/списка; их анализ остаётся в EF-27.

Saved preparation читает только выбранный current plain файл. Broken/missing
neighbor values не требуют обхода всего рабочего проекта для правки этого
файла. API не меняет source XML references или `form.yaml`. Полный export
отдельно проверяет actual source set, profile, slots и inverse recipe.

Подготовка отказывает для missing/non-UTF-8, read-only и symlink файлов.
Перед atomic replacement общий writer проверяет exact original bytes,
independent base pin, неизменность pinned storage и Resource permission.
Conflict/base substitution/corruption сохраняют current файл. Byte no-op
сохраняет mtime. Финальная проверка bytes не является OS compare-and-swap
против некооперирующего процесса между проверкой и rename.

Шесть owned tests проверяют condition edit для СКД и списка, byte no-op,
unsaved/empty text без neighbor reads, поддержку/freeze/подмену declaration,
недопустимые symbols/размер и repair, conflict/pin/storage corruption,
реальные symlink/read-only/non-UTF-8/missing отказы. Condition edit меняет
ровно один plain файл; independent native codec и protected export совпадают
и меняют только соответствующий query property одного ресурса.

[C1 results](query-edit-results.json) фиксируют проверку настоящих supplier
прав отдельно от fixtures: 46 previews разрешены, 104 отказаны. Два saved edits
(СКД и список) изменили только свои plain files и два native ресурса; полный
output равен independent control по 8 769 файлам/8 064 каталогам. Accepted storage
прежний. Девять gates прошли: 700 unit, 570 integration, 359 SDK tests;
24 прежних ignored. Явная миграция и новый платформенный цикл ещё нужны.
Query runtime этой частью не принимается; EF-23 остаётся `IN_PROGRESS`.
