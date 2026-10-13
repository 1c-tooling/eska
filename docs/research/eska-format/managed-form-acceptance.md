# Матрица управляемых форм и приёмка EF-12

Проверено 2026-10-13. EF-12 принята для configuration, native logform 2.20,
на 1С 8.3.27.2325. Основание — матрица 823 форм C1, общий reader/codec/editor,
защищённая материализация и независимый XML → CF → свежий dump изменённой формы.
Это приёмка модели форм; файловый цикл четырёх артефактов и весь MVP ещё в работе.

## Единственный источник значений

[Карта версии 4](managed-form-project.md) связывает metadata descriptor и native
dialog по прежнему metadata UUID и полным native paths. Они имеют один `form.yaml`:
readable descriptor schema 2 и body schema 2. Definitions находятся в `elements`,
порядок и parentage — в `layout`; properties не повторяются в native stencils.
Модуль и остальные ресурсы сохраняют отдельные файлы. Native details нужны
для обратной записи, не становятся альтернативным источником editable values.

Импорт forms и [миграция](managed-form-migration.md) доступны как explicit core APIs.
Opening не мигрирует проект, CLI defaults прежние. Working source и independently
pinned accepted base имеют разные планы; миграция не принимает текущие правки.
Прежние layouts сохраняют чтение; прежние migration targets сохраняют map 4 без downgrade.
[Query target EF-23](query-migration.md) явно переводит map 4 в map 5, сохраняя body.

## Принятое и сохранённое покрытие

| Область | Проверенное поведение | Граница |
|---|---|---|
| Identity и scopes | [52 502 UI ID, 4 971 Attributes, 2 303 Commands, 2 028 columns](managed-form-inventory.md); UUID формы и numeric scopes независимы | Definition/ID/name/class create/delete/rename не приняты |
| Layout | [Один parent, nesting, reachability, cycles, 29 UI classes](managed-form-layout.md); порядок и допустимый parent ordinary элементов меняются отдельно от properties | Fixed edges и ближайший Table context сохраняются; неизвестный profile отказывает |
| Fixed slots и ссылки | AutoCommandBar, menus/tooltips/additions; 1 822 AdditionSource и 2 804 local commands разрешены в C1 | DataPath, builtin/numeric commands и неизвестные reference shapes не получают придуманную семантику |
| Readable properties | [Title, ToolTip, Visible, ReadOnly, Enabled](managed-form-properties.md); 12 317 properties, один value, XML escaping и существующие языки/flags | New/absent slots, новые языки и смена вида property запрещены; unknown shapes сохраняются opaque |
| Attributes/Commands/events/settings | Их identity/reference context и исходные XML bytes сохраняются общим [body codec](managed-form-body.md) | Произвольные внутренние правки не приняты; запросы выделяет EF-23 |
| Metadata descriptor | Прежние typed metadata values и native identity в том же YAML | Права metadata и body выбираются независимо по принятому UUID |
| Supplier/freeze | [Protected selection](managed-form-protected.md) выбирает разрешённые Resource values/layout; запрещённое восстанавливает из базы | Working map/stencils/UUID не дают authority; strict отказывает до publication |
| Editor/source map | [CST edits](managed-form-edit.md), shared atomic writer и map 3 с byte-verified Working/Accepted spans | Неподдержанная trivia, stale bytes/pin и invalid layout отказывают; visual editor/merge не входят |
| Binary forms/orphan resources | Сохраняются исходными payload bytes | Типизированное редактирование обычных двоичных форм не заявлено |

## Сверка DoD

| Критерий EF-12 | Подтверждение |
|---|---|
| Реальная матрица ID/ссылок | Все 823 declared dialogs C1; independent UI/Attribute/Command/column scopes и native reference fixtures |
| Единственные definitions/layout/values | Combined schema; saved/overlay reader; XML → YAML → XML всех форм сохраняет native dialog bytes |
| Parent/nesting/reachability/cycles/collisions | Общий валидатор; valid moves и явные отказы по fixed/native/namespace/Table contexts |
| Изменение layout без смены identity | Массовый codec reparse и protected selection; два saved edits реальной формы сохранились после платформы |
| Повторный импорт/миграция | Native UUID/ID берутся из input, не генерируются заново; repeat codec/import/export, independent source/base plans, no-op/repeat/resume/conflicts и workspace pins |
| Writer, support/freeze, source map | Принятый UUID, Resource permission, strict/recovery, отдельный origin dialog и точные CST координаты |
| Реальный экспорт и платформа | Independent native control и ESKA build/dump на установленной 1С; полное сравнение ниже |
| Unknown/binary границы | Opaque bytes сохраняются; unsafe/непроверенные changes отказывают до publication |

## Реальный CF цикл

[Результаты](managed-form-acceptance-results.json). Миграция readable C1 создала
map 4 с 823 формами и отдельно восстановленной принятой базой. Все Resource
permissions сохранились: 186 разрешённых, 637 запрещённых. Native no-op совпал
с независимым прежним readable export по 8 769 файлам и 8 064 каталогам.

В одной разрешённой форме через saved editor изменён localized root Title
и изменён порядок root layout. Strict protected export прошёл без exclusions,
изменился только её dialog XML. Контроль подготовлен отдельно из прежнего
native export: Python Expat адресовал Title и native UI ranges, не используя
YAML parser, ManagedFormSource или form writer ESKA. До платформы оба полных
native дерева совпали по bytes и paths.

Оба маршрута собраны через `eska-build build` в собственных playground проектах
и выгружены через `eska new --from` в новые каталоги. Runner — Distrobox,
платформа 8.3.27.2325. Artifact checksums сверены с build manifests; ESKA manifest
содержит source map 3/descriptor schema 2 и выбранный accepted pin.

Свежие dumps совпали по всем 8 769 файлам/8 064 каталогам, кроме 7 011
`ConfigDumpInfo.xml` configVersion. Допущены только проверенные version attributes;
остальные bytes этого файла тоже равны. Независимый parser подтвердил новый Title
и порядок native IDs после платформы. Source/base/settings/pin и версия проекта
сохранены. Побайтовое равенство двух CF containers не требуется и не заявляется.

Последние общие gates относятся к коду миграции: fmt/check/strict Clippy/test/
Rustdoc, 661 unit/570 integration; SDK all features check/Clippy/test/Rustdoc,
359 тестов; project/build CLI 44/9. Для этой документации проверены ссылки,
JSON и diff; повторный Cargo-прогон не запускался.

EF-12 — `DONE` в указанном профиле. C2/C3, CFE/EPF/ERF, визуальная проверка
клиента и выполнение формы этой приёмкой не подтверждаются. Generic UI edits,
неизвестные profiles и binary authoring остаются вне матрицы. Следующая задача
MVP — EF-23, хранение и материализация запросов СКД/динамических списков в `.sdbl`;
анализ этих запросов остаётся отдельной EF-27.
