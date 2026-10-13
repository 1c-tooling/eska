# Защита и координаты отдельных запросов — EF-23, часть 4

`prepare_protected` принимает [project map 5](query-project.md). Прежний
`source_query_protection_unavailable` удалён. Разрешения определяются metadata
UUID формы/шаблона из принятой карты и операцией `Resource`; рабочие пути,
UUID, `source.json` и ссылки не дают права на редактирование. Native support
и freeze владельца или его metadata предков сохраняются.

## Выбор значений

Разрешённый plain input выбирается из captured working bytes. Его отсутствие
даёт `source_query_value_missing` до merge/publication, независимо от strict.
Прежний inline query не подставляется. Запрещённая правка или потеря plain файла
восстанавливается из accepted snapshot с `Resource` diagnostic; bytes кандидата
не интерпретируются как разрешённый UTF-8/запрос. Strict отказывает при exclusions.

Форма сохраняет pinned native definitions/stencils, включая query references;
принятый EF-12 selector независимо выбирает разрешённые properties/layout.
Plain значения выбираются отдельно. Подмена form UUID, reference namespace
или raw binding восстанавливается с диагностикой, не стирая независимых
разрешённых правок. Для СКД весь XML resource выбирается по прежним Resource
правам, затем проверяется actual profile, declared source set и native scopes.
Переименование набора в разрешённом СКД сохраняет путь existing reference;
сам filename не становится identity.

Все выбранные ссылки и значения читаются через один captured inventory.
Полный materialized XML вычисляется до generation; fingerprints рабочего
дерева и base повторно проверяются при публикации. Missing/foreign/colliding
ссылка, inline fallback, неверный protocol/profile и XML characters дают отказ.
Прежние immutable generations и рабочие файлы сохраняются.

Консервативный guard существующих metadata rename/delete теперь проверяет
выбранные plain values: извлечение текста не скрывает ранее обнаруживаемую
зависимость. Это прежняя ограниченная проверка tokens, не анализ языка СКД
или согласованности его полей/потребителей; полный анализ относится к EF-27.

## Source map 4

`QUERY_SOURCE_MAP_VERSION = 4` описывает composite resources при map 5.
`source_schema_version = 2` по-прежнему описывает descriptor envelope, не
обратную карту. Предыдущие maps 1–3 сохраняют wire output и чтение.

`managed_form` получает optional `queries`; при пустом наборе поле не выдаётся.
Новый origin `query_resource` содержит accepted metadata `uuid`/`descriptor`,
выбранный source XML `authority`/`input` и `queries`. Каждый `QueryOrigin`
содержит actual `slot`, полный native property `native` range и независимо
выбранные `authority`/plain `input` с размером/SHA-256. СКД без queries тоже
получает composite origin с пустым набором. Metadata descriptor, form properties,
resource XML и query values могут иметь разные selected-value состояния.

DTO validation проверяет version, связь с metadata UUID/descriptor, owner prefix,
уникальность plain paths/scoped slots, resource class и непересекающиеся native
ranges. Карта описывает происхождение конкретной generation; десериализация
и checksum не создают `PreparedSource` и не разрешают сборку.

`MappedFile::locate` проверяет native fingerprint и actual slots/ranges. Plain
input подтверждается SHA-256 decoded native text; дополнительное чтение `.sdbl`
для адресного mapping не нужно. Byte span переводится в UTF-8 offsets plain
файла с учётом entities, numeric CR, XML CR/CRLF normalization, CDATA delimiters
и split `]]>`. Unicode character и entity атомарны; mid-UTF-8 spans отказывают.
Decoder проходит до запрошенного места без хранения массива для каждого символа.
Native header может дать только file-level plain location; пустой query — `0..0`,
если весь запрошенный span находится внутри его native node. Диапазон, выходящий
из пустого узла в соседний XML, получает только file-level location без точного
plain range.

Вне queries адреса СКД указывают на выбранный source XML. При supplied exact
`OriginTexts` проверяется inverse и учитывается сдвиг от замены reference nodes;
без него результат file-level. Body properties/layout формы по-прежнему
указывают на accepted/working YAML CST. Mapper возвращает query nodes к pinned
references перед проверкой stencils и корректирует offsets после query blocks.
Нет переноса XML line number непосредственно на YAML.

## Проверки и граница

Семь новых owned project tests проверяют разрешённые edits и composite DTO,
exact UTF-8/entity/CR locations, независимые form properties после queries,
restoration подменённых bindings/UUID, native policy/inherited freeze,
missing allowed values, повреждённые references/characters, forged map/slots/
fingerprints и прежний metadata dependency guard. Два pure tests добавляют
split CDATA, empty query, changed-byte отказ и точные границы соседних nodes.
Прежний temporary refusal test заменён protected no-op/control сравнением.

[C1 results](query-protected-results.json) фиксируют supplier selection и полную
native сверку отдельно от синтетических fixtures: 46 values разрешены, 104
восстановлены; изменён ровно 41 ресурс. Полный output равен независимому контролю
по 8 769 файлам/8 064 каталогам. Проверены все 150 origins и разрешённые spans,
strict отказ, repeat generation и сохранность inputs. Прошли Cargo/SDK gates:
694 root unit, 570 integration и 359 SDK tests; 24 прежних ignored.
Точечный query editor, явная source/base/project/workspace migration и новый
платформенный цикл ещё нужны. CF/build/client execution изменённых запросов
этой частью не принимаются. EF-23 остаётся `IN_PROGRESS`.
