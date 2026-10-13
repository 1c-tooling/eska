# Версионированные ссылки на запросы — EF-23, часть 2

Pure API `project_core::source_format` добавляет `NativeQueryDocument::extract`,
`ExtractedQuerySource`, `QueryReferenceDocument` и `QueryReference`. Caller
явно задаёт `QuerySlot → SourcePath`; extraction передаёт единственный decoded
text в набор plain sources. Это ещё не project import/export: schemas 1–4,
CLI defaults, reader и trusted base не менялись.

## Протокол source XML

Версия ссылки задаётся namespace `urn:eska:query:1`, независимо от схемы проекта.
Пример внутри native query property:

```xml
<query><eska:query xmlns:eska="urn:eska:query:1" source="Queries/Основной.sdbl" coding="text" eol="crlf" cr="13" quotes="false" apostrophes="false" empty="false"/></query>
```

Namespace локален marker. В родительском native XML его объявлять нельзя:
materialization удаляет весь marker, включая namespace. Допускаются другие
prefix aliases с тем же URI. Все семь атрибутов обязательны; неизвестные
атрибуты, дочерние узлы, inline text, XML comments/PI внутри property и
несогласованная версия отказывают. Пробелы вокруг marker допустимы и не являются
значением запроса. Comments и прочие bytes вне query property сохраняются.

| Поле | Значение |
|---|---|
| `source` | Portable relative `SourcePath` с расширением `.sdbl`, без traversal/control characters; регистр расширения несущественен |
| `coding` | `text` или `cdata` |
| `eol` | Native XML transport: `lf`, `crlf`, `cr` |
| `cr` | Native representation actual CR: `13`, `xD`, `xd` |
| `quotes`, `apostrophes` | `true`/`false`, наличие native entity style |
| `empty` | `true`/`false`, исходный self-closing native query |

Для `cdata` quote flags должны быть `false`, CR style — `13`. Recipe хранит
только обратное кодирование; прежнего query text в нём нет. Точные native
opening/closing headers остаются вокруг marker. Их происхождение и supplier
authority pure parser не доказывает: это задача проектного слоя с accepted pin.

Один source соответствует одному slot; одинаковые и case-aliased пути запрещены
в пределах документа. Контекст slot разбирается из действительного native
dataset/union либо form Attribute scope теми же правилами, что в
[native codec](native-query-codec.md). При rename dataset существующая ссылка
сохраняет путь, а native binding отражает новое имя. Это не операция безопасного
переименования проекта и не разрешение менять pinned native structure.

## Материализация и проверки

`QueryReferenceDocument::materialize` требует ровно все заявленные plain values:
missing/extra sources отказывают без прежнего inline fallback. Values проверяются
на XML characters. Результат повторно разбирается как native XML, каждый scoped
binding/text сверяется. Native codec отказывает при source protocol namespace;
смешанный native/reference документ не принимается. Файловый resolver, безопасное
чтение диска и project naming convention этот pure API не предоставляют.

[C1 результаты](query-references-results.json): 823 forms/51 schemas, 150 файлов
для 27 запросов СКД и 123 списков в 126 ресурсах. Plain values записаны и прочитаны
с диска; все no-op XML совпали побайтно. Изменены все 150 файлов, обратный разбор
подтвердил точные values. Missing/extra values отказаны, namespace в native output
отсутствует, исходная выгрузка прежняя. CF/build/execution изменённых запросов
этой частью не принимались.

Восемь новых tests проверяют single value, lexical recipes/CDATA/empty/EOL,
union scopes/rename, DynamicList scopes, отсутствие файлов, case collisions,
неверные версии/recipes/markup и namespace в opaque XML. Cargo и SDK gates
зафиксированы в results. Следующая часть — project paths/reader/import/export,
защита/source map и миграция. EF-23 остаётся `IN_PROGRESS`, language scope — EF-27.
