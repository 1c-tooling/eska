# Обратимое содержимое формы — EF-12, часть 3

`ManagedFormSource` добавляет обратимый codec одного managed dialog поверх
[общей identity/layout модели](managed-form-layout.md). `from_xml`, `to_yaml`,
`from_yaml` и `to_xml` используют единственные definitions и ordered layout.
Схема экспериментальная: [body v1](managed-form-body.schema.json),
[синтетический пример](managed-form-body.example.yaml).

Это подготовка единого `form.yaml`. Текущий project import/file map ещё хранит
descriptor отдельно от XML dialog; новая версия manifest, protected selection,
миграция и source map пока не подключены. В пользовательском проекте дополнительный
body-файл или editable XML копия этой частью не создаются.

## Значения и native bindings

`elements.<ID>` содержит class/name ровно один раз, `layout` — порядок/родителей
обычных элементов. `native` содержит плоские stencils с XML-фрагментами и ссылками
на definitions, fixed elements и Children slots. UI class/name/ID и состав
ChildItems не хранятся внутри второго полного XML. UUID metadata descriptor
не заменяется decimal UI ID.

Attributes/Commands/Columns, properties, events, CommandInterface и настройки
пока сохраняются в opaque фрагментах. Data identity/known references проверяются
общим native reader при материализации. Например, CommandBar без ID внутри
CommandInterface — сохранённый раздел, не определение UI.
Title/ToolTip и bool properties ещё не вынесены в editable definitions.

Плоские stencils отделены от graph: глубина UI не увеличивает вложенность YAML.
Writer использует work stack после полной проверки parents/cycles/nesting.
Definitions, binding keys и वास्तविक Children slots должны совпасть; скрытые
XML definitions или потерянный child не являются fallback.

## Разрешённые layout proposals

Порядок обычных элементов меняется только в `layout`. Перенос между существующими
ChildItems slots меняет parent, сохраняя definition и fixed owners. Native empty
`<ChildItems/>` может развернуться. Отсутствующий слот автоматически не создаётся:
такой перенос возвращает `source_form_layout_slot_invalid` до материализации.

Все gaps/comments сохраняются у прежнего owner ровно один раз, даже если число
детей изменилось. Existing sibling gaps не становятся копией всего subtree.
Fixed owner bindings сравниваются по ID/owner, независимо от порядка native обхода;
перестановка обычных siblings не превращается в подмену fixed slot.

Прежний nearest-Table context сохраняется. Дополнительно проверяется effective
prefix-to-URI scope каждого UI node: перенос opaque содержимого между различными
namespace scopes возвращает `source_form_namespace_context_changed`.
Native stencils и definitions сравниваются с принятой моделью при
`validate_layout_change`. Это не permission check поддержки: публичная сборка
должна пройти отдельный protected selector формы по её metadata UUID.

## Точность и адресное чтение

Импорт codec требует byte-equal no-op XML: BOM, CRLF, декларации, comments,
CDATA и непринятое содержимое не нормализуются. Непринятая lexical запись
typed identity, например numeric character reference в `id`, возвращает
`source_form_lexical_unverified`; прежний raw/layout reader доступен.

`SourceReader::read_form_source` читает только declared dialog с прежним byte limit
и проверкой пути/кодировки. `parse_form_source` принимает unsaved native XML
без filesystem чтения. Body YAML проходит строгий общий parser: duplicate keys,
unknown fields и подмена identity holes отвергаются.
CLI/config/JSON protocols и текущие project schemas не менялись.

## Проверки

Все 823 declared dialogs C1 прошли byte-equal XML → source YAML → XML,
включая 52 502 UI definitions. Saved inputs не менялись. Для 665 форм с несколькими
корневыми элементами проверены перестановка, reparse и прежние fixed bindings.
Получено 52 977 350 bytes body YAML суммарно; это объём последовательной
сериализации, не новый project footprint/замер производительности.
[Обезличенные результаты](managed-form-body-results.json).

Девять новых tests проверяют BOM/CRLF/CDATA/unknown content, id-less CommandBar,
quoted tag punctuation, sibling reorder, moves/empty slots/comments, namespace
context, pinned bindings, cycles, strict YAML и lexical refusal. Адресный reader
test дополнен saved/overlay codec без существующего файла.
Синтетический пример проверен по JSON Schema Draft 2020-12.

Полные fmt/check/strict Clippy/test повторены после уточнения CommandInterface;
также прошли all-features/minimal/project/build CLI, Rustdoc и SDK/adapter gates.
Приложение: 613 unit/570 integration, 24 прежних opt-in ignored; CLI: 44/9;
SDK: 359; adapter: 70 unit/12 integration. C2/C3 и новый CF/client/runtime цикл
этой частью не выполнялись. Следующее — readable properties и подключение
body к единому form.yaml, затем protected writer/migration/source map.
