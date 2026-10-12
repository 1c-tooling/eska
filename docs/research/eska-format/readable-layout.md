# Именованные свойства YAML, пятая часть EF-24

Descriptor schema 2 отделяет обычные значения от служебного восстановления XML.
Обратная карта остаётся версии 3, conversion rules и native profile — версии 1.
Публичный CLI импорта пока создаёт прежнюю семейную схему; открытие старого
проекта ничего не переводит.

## Представление

Ниже показана только обычная часть descriptor, без обязательной служебной части:

```yaml
schema_version: 2
rules_version: 1
profile: "designer-2.20-configuration"
object:
  kind: Catalog
  uuid: "20000000-0000-4000-8000-000000000001"
  properties:
    Name: "Sample"
    Comment: {state: empty}
    CodeLength: "0009"
    Hierarchical: false
  children: []
```

Файл сохраняет wrapper `document.path`/`document.descriptor` и пустой `nested`.
Именованные `properties` содержат единственную редактируемую копию значения.
`native_xml` сохраняет профиль XML, BOM/EOL и root namespace bindings;
`object.native` — namespaces, attributes и native sections. Служебные slots
properties содержат только имена typed-свойств или opaque fragments, без второй
копии значений. Slot children указывает место секции; сами children объявлены
один раз, в смысловом порядке. Inline descendants используют ту же оболочку.

Writer YAML ставит identity и обычные свойства перед native details.
JSON schemas: [descriptor](descriptor-readable.schema.json),
[самостоятельный источник](object-source-readable.schema.json). Core дополнительно
проверяет точное соответствие slots/значений, UUID, namespace context, native
порядок, owner-specific домены и правила preserved XML.

Матрица [32 typed-свойств](typed-scalars.md) не расширяется этой оболочкой.
Строки и enum — строки, boolean — bool, integer — точная строка ASCII-цифр.
Числа с ведущими нулями и произвольным количеством цифр не проходят через
float/u64. `Synonym` — упорядоченная последовательность language/text.
Empty и nil задаются только `{state: empty}` и `{state: nil}`; YAML null,
пустой текст и неподходящие native domains не принимаются. Отсутствие значения
отличается от обоих состояний. Writer не создаёт defaults.

Нельзя добавить именованное значение без native binding, повторить имя slot,
убрать привязанное значение или выдать preserved XML за typed-копию.
Состав и права не определяются сканированием каталогов либо рабочей identity.
Неизвестные XML-фрагменты, включая собственные namespaces и QName-values,
остаются в прежнем slot и не становятся типизированными при миграции.

## Общие потребители

Core использует одну lossless модель метаданных. Сериализация выбирает только
wire envelope; native writer, policy и семантические значения общие для версий.
Reader/declarations/semantic projection, CST editor и source coordinates используют
общие schema paths. Свойства версии 2 адресуются по именам; неизвестные фрагменты
сохраняют native ordinal. Inline identity и logical parents не меняются.

CST-правка заменяет выбранный direct scalar, state wrapper или localized text.
Соседние байты, комментарии, CRLF и языковой состав остаются прежними. Для строки
с переводом строки выбирается quoting, сохраняющий точное значение. Необработанное
оформление отказывается до записи. Equal-value edit не публикует файл.

Protected export читает разрешённые значения по именам, но выбирает schema,
kind, UUID, состав и права из independently pinned base. Запрещённая raw-правка
восстанавливается с операцией `property:<Name>`; strict отказывает при exclusions.
Редактирование slots, UUID или schema не даёт разрешений.

Source map schema 2 использует `source_schema_version = 2` для этих inputs;
layout 3 с прежними descriptors сохраняет значение 1. Карта версии 1 не принимает
новую descriptor schema. Координаты typed values показывают direct value;
preserved XML — служебный fragment. Fingerprints проверяются до разбора текста.
SDK использует ту же semantic projection; новый analyzer protocol не вводится.

## Явная миграция

`import_designer_readable` создаёт layout 3/descriptor 2 из native XML.
`prepare_readable_layout` строит expected-byte план исходников: отделяет descriptors,
переносит resources и переводит envelope. `prepare_accepted_readable_layout`
читает только pinned blobs и возвращает отдельный новый pin. Рабочие правки
не становятся принятой базой; settings/CI pins сами не меняются.

Публикация использует существующий exclusive publisher: входные fingerprints,
новый owned output, ready marker последним, repeat/no-op и процессное recovery.
У уже читаемого descriptor сохраняется exact text. Перевод старого envelope
пока отказывается при YAML comments: нет молчаливой потери аннотаций. LF/CRLF
переносятся, значения и native XML проверяются независимо. CST-миграция legacy
comments остаётся отдельным незавершённым пунктом EF-24.

## Проверки и границы

Прошли 15 root/14 SDK gates: fmt/check/strict Clippy/test/Rustdoc, minimal и
отдельные CLI. Root — 410 unit + 540 integration, 20 opt-in пропущены обычным
запуском. SDK all-features — 359 тестов. Проверены strict bindings/типы, точные
числа, unknown XML, empty/nil, native bytes, две pinned/source migrations,
комментарии/CRLF правок, поддержка/freeze, raw bypass и coordinates typed/preserved.
Новый SDK-тест проверяет RU/EN completion, unsaved flags и BSL navigation
через реальное ядро с физическими путями.
Адаптер BSL прошёл отдельные fmt/check/strict Clippy/test/Rustdoc: 70 unit и
10 integration. Исправлены два устаревших fixture, ищущих прежние XML-фрагменты
типизированных свойств; несохранённые common module/catalog drafts и точные
диагностические координаты проверены на обеих descriptor schemas.

Реальная C1 сохранила 3 595 descriptors/8 330 объектов и 33 320 policy decisions.
Все 5 183 payload совпали; fresh import и обе миграции прошли полную `All`
сверку. Правка одного Comment меняет один YAML и один native descriptor;
независимый XML-контроль совпал по всем байтам. Schema валидирует все 3 595 YAML.
После перехода из семейного layout размер YAML составил 34 929 113 байт против
46 670 498; это измерение одного корпуса, не гарантия производительности.

Контроль и readable проект собраны в CF и повторно выгружены через ESKA на
1С 8.3.27.2325. Совпали 8 769 файлов/8 064 каталога, кроме прежних 7 011
`ConfigDumpInfo.configVersion`. Artifact checksums, новый pin и source map
schema 2/descriptor version 2 проверены; exclusions отсутствуют. Исходный XML,
старые/новые accepted bases, build inputs и версия конфигурации не изменились.
[Обезличенные результаты](readable-layout-results.json) отделяют платформенный
цикл от выполнения в клиенте и новых профилей.

Составные типы, новые ссылки/коллекции, structural create/delete/rename,
публичная orchestration и перенос legacy annotations ещё не выполнены.
Новые C2/C3, CFE/EPF/ERF и клиентское выполнение этой частью не принимаются.
EF-24 остаётся `IN_PROGRESS`.
