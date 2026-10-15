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
рендеринг формы сохраняет прежний output; trace собирается только для read-view.
Quoted и literal YAML проверены; folded scalars отвергает существующий decoder.

Прошли 64 профильных теста: 7 новых context, 30 form, 19 query codec и 8 query
layout. Включены CF/CFE/EPF/ERF XML fixtures, отличающийся native stem, формы и СКД,
current strings после удаления source tree, неверные identity/profile/links,
границы объёма и координат. Default/all-features check, strict lib/tests Clippy,
scoped fmt и diff check прошли. Это Rust fixtures; новый платформенный цикл
этой частью не запускался.

SDK query slots, parser/resolver и проверки потребителей ещё не подключены.
Для EF-27 нужны текущий `.sdbl`, параметры/поля/настройки, selected metadata
visibility, dependency invalidation и IDE/batch parity. Для generated form
properties и decoded YAML subranges потребуется отдельное доказанное mapping.

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
