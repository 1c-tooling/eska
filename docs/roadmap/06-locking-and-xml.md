# Locking и Designer XML

## T19 — Designer XML logical object model

**Статус:** `DONE`
**Зависит от:** T03

Реализовать discovery объектов, стабильный `ObjectId`, metadata type/name,
module/form paths, mapping object → paths и changed path → objects. Проверить на
configuration/extension/processing/report fixtures.

**Производительность:** не выполнять обязательный полный parse для каждой команды;
`.eska/cache|index|state` добавить только после подтверждённой необходимости и не
хранить в VCS.

Принятые решения T19:

- `project::object_model::discover` явно строит read-only модель по требованию;
  остальные команды не получили обязательный полный parse, cache/index/state не
  добавлены;
- `ObjectId` — детерминированный читаемый путь из machine-facing типа и имени,
  например `catalog:Partners/form:Item`; разделители в именах percent-encoded.
  Designer UUID сохранён как дополнительное поле, но не является идентификатором:
  реальные выгрузки могут повторять UUID у разных объектов;
- namespace-aware parser обнаруживает корневые, верхнеуровневые и вложенные
  объекты, включая inline `ChildObjects` и отдельно выгруженные формы, команды,
  макеты и рекурсивные подсистемы. Два представления одного объекта объединяются;
- модель хранит тип, имя, UUID, родителя, основной descriptor, все связанные
  source-relative paths, модули и формы. Доступны оба направления mapping:
  object → paths и changed path → ближайшие objects;
- обход детерминирован, ограничивает descriptor 64 МиБ и XML 1 000 000 узлов,
  не выходит по symlink за source и не разбирает payload XML как metadata
  descriptor;
- fixtures покрывают configuration, extension, external processing и external
  report, inline children, формы, модули, malformed descriptor и повтор UUID.
  Дополнительно полная реальная configuration выгрузка успешно построила модель
  из 8 013 объектов.

## T39 — Locking конфликтных объектов

**Статус:** `PLANNED`
**Зависит от:** T07, T19, T38

Команды `lock <ObjectId>`, `unlock <ObjectId>`, `locks`. Пользователь указывает
логический объект (Form, Role, DataCompositionSchema и policy-defined types), не
XML path. Первичный backend может использовать Git LFS/server locks, но backend не
входит в public API. Designer XML не переводится в LFS storage только ради locks.
Unlock при неопубликованных изменениях отказывает; `--force` явный.
