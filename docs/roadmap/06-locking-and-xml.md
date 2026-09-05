# Locking и Designer XML

## T19 — Designer XML logical object model

**Статус:** `NEXT`
**Зависит от:** T03

Реализовать discovery объектов, стабильный `ObjectId`, metadata type/name,
module/form paths, mapping object → paths и changed path → objects. Проверить на
configuration/extension/processing/report fixtures.

**Производительность:** не выполнять обязательный полный parse для каждой команды;
`.eska/cache|index|state` добавить только после подтверждённой необходимости и не
хранить в VCS.

## T39 — Locking конфликтных объектов

**Статус:** `PLANNED`
**Зависит от:** T07, T19, T38

Команды `lock <ObjectId>`, `unlock <ObjectId>`, `locks`. Пользователь указывает
логический объект (Form, Role, DataCompositionSchema и policy-defined types), не
XML path. Первичный backend может использовать Git LFS/server locks, но backend не
входит в public API. Designer XML не переводится в LFS storage только ради locks.
Unlock при неопубликованных изменениях отказывает; `--force` явный.
