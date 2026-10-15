# Текущий ресурс контекста запроса

`SourceReader::parse_query_context` принимает native resource path и точные
current owner/resource strings. Для layouts 5/6 проверяются manifest ownership,
UUID, descriptor family, resource profile и весь набор scoped query links.
Метод не читает файлы, accepted storage или соседние `.sdbl`, не анализирует
язык запроса и не выдаёт разрешение на запись.

`QueryContextView` сохраняет owner/native descriptor/UUID, единственный resource
source path, `QueryProfile`, ссылки `QuerySlot` → `SourcePath` и bounded reference
XML. Если descriptor и resource находятся в одном `form.yaml`, supplied strings
должны совпадать побайтно. Current DCS descriptor должен оставаться макетом
DataCompositionSchema; иной XML root не подменяет объявленный профиль.

`locate_reference_range` различает точность координат:

- `Exact` — диапазон исходного DCS XML;
- `EnclosingNode` — точный CST scalar текущего YAML, породивший сохранённый XML
  fragment; decoded substring offsets после escaping не заявляются;
- `Unmapped` — generated value, несколько fragments или отсутствие единственного
  доказанного source node.

Обратные и неверные UTF-8 byte ranges отклоняются. Renderer trace использует
фактические part indices и native element IDs, без поиска подстрок. Обычный
рендеринг формы сохраняет прежний output; trace нужен для координат read-view.
Quoted и literal YAML проверены; folded scalars отвергает существующий decoder.

Прошли 64 профильных теста: 7 новых context, 30 form, 19 query codec и 8 query
layout. Включены CF/CFE/EPF/ERF XML fixtures, отличающийся native stem, формы и СКД,
current strings после удаления source tree, неверные identity/profile/links,
границы объёма и координат. Default/all-features check, strict lib/tests Clippy,
scoped fmt и diff check прошли. Это Rust fixtures; новый платформенный цикл
этой частью не запускался.

SDK теперь использует текущие query/owner/resource inputs, как описано ниже.
Для EF-27 ещё нужны параметры/поля/настройки, consumer diagnostics и полная
IDE/batch parity. Для generated form properties и decoded YAML subranges
потребуется отдельное доказанное mapping.

## Потребители в текущем ресурсе

`QueryContextView::consumer_view` читает storage facts из этих же supplied bytes:
dataset scopes Query/Union/Object, явные `DataSetFieldField` bindings,
DynamicList attribute/name/manual-query/main-table и UI `DataPath`. Полная
union ancestry сохраняется; attribute и UI IDs остаются разными пространствами.
QName определяется по namespace, а не написанию prefix.

Каждый `QueryLocated` хранит decoded value, проверенный native node/attribute
range и `QueryAuthoringRange`. Whole node либо enclosing YAML scalar не выдаётся
за decoded substring. Missing/duplicate/empty поддержанные bindings отказывают;
параметры, связи, ресурсы/итоги, calculated fields, настройки и unknown shapes
остаются locatable inventory с `Incomplete` или `Unsupported` coverage.
`consumer_complete` всегда false: это чтение структуры, без query output/type
inference, resolver или consumer diagnostics.

Прошли 25 query codec/consumer tests и 7 current read-view tests, включая шесть
новых consumer groups на всех четырёх типах. Проверены nested unions, namespaces,
current YAML после удаления source tree, отдельные ID scopes, invalid bindings
и доказанные координаты. Minimal/all-features lib check и strict lib/tests Clippy,
scoped fmt и diff check прошли. Новый платформенный цикл не запускался.

## Один текущий источник в SDK

`MetadataSnapshot::form` захватывает `CurrentFormView` при разборе текущего
owner. Reference XML и context используют один `Arc<str>`; исходный YAML не
удерживается второй раз. Unified `form.yaml` не регистрируется отдельным dialog
resource: native BSL form projection получает обработчики и элементы из этого
же snapshot. Прежний отдельный XML dialog сохраняет свой resource lifecycle.

Неверные query links отказывают context read-view, сохраняя отдельно доказанную
metadata model формы. Во время projection не материализуются `.sdbl` и не
перечитывается сохранённый owner. Это исправляет случай, когда dirty handler
уже принят в YAML, а native unused-method diagnostics ещё используют старый.

Отдельный XML макета СКД имеет `MetadataResourceKind::QueryComposition` в том же
current input controller: origin/hash/version, лимиты, failed overlays,
watch/close/import retirement. Bounded raw read не требует соседних запросов
или повторного разбора owner. Malformed XML остаётся текущим raw input, а
consumer parser возвращает отказ. Pending consumer-only СКД не выдаётся за
неготовность canonical BSL metadata.

`prepare_query_context` подготавливает один запрос, его owner и отдельный
resource. `query_context` возвращает `QueryConsumerContext` с global revision,
selected root, composition hash, существующими owner/resource/query revisions
и точными source/hash/origin. Unified resource разделяет owner authority и его
reservation. Pure `parse_query_context_from_snapshot` проверяет отдельную СКД
по уже принятому metadata snapshot; saved fallback отсутствует.

Это экспериментальные library APIs. Новый RPC для read-view не добавлен;
querySyntax/queryAnalysis diagnostic payloads не изменены. Чтение storage
bindings сохраняет `consumer_complete = false` и не принимает output types,
parameters, настройки или всю семантику СКД.

Проверены current form handlers, отсутствие hidden saved query reads, отдельная
DCS sync на CF/CFE/EPF/ERF, invalid/version/watch/close/recovery, единая reservation
и сохранность текущих hash/origin. Полный SDK: 197 default и 488 all-features tests,
ноль ignored; оба check/strict Clippy/Rustdoc и fmt прошли. Debug/release analyzer
и checker собраны при неизменившихся 1423 Rust/Cargo files. Core прошёл полный
fmt/check/Clippy/Rustdoc и 1508/1629 tests с 24 прежними opt-in пропусками;
13 context tests включают новые no-filesystem, bounds и symlink проверки.

Свежая immutable release-пара прошла 13 runtime groups с пустым `PATH`:
native CF handler swap, current invalid/close, удалённые saved neighbor query
и DCS, all4 current XML/version/watch/close/repair и shared-quota refusals.
Для запрошенного native CF query IDE/checker совпали по codes/ranges/hash на
RU/EN; checker сохраняет exit 2 из-за непринятых consumer semantics. Отдельные
native-origin CF/CFE/ERF DCS controls тоже прошли; all4 fixtures не выданы за
платформенный цикл. Источники сохранены по bytes/mode/mtime, завершены 34 analyzer
и 4 checker children. Первый неверный harness expectation сохранён отдельно;
intrinsic BSL findings при invalid metadata допустимы только с явной неполнотой.
[Обезличенные результаты](current-query-resource-results.json).
