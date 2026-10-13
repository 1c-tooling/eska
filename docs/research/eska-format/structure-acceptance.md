# Матрица исходников и приёмка EF-24

Проверено 2026-10-13 по коду, актуальному DoD и результатам экспериментальной
ветки. EF-24 принята для `designer-2.20-configuration` на 1С 8.3.27.2325.
Формы, запросы и три дополнительных вида проектов имеют отдельные критерии
EF-12/EF-23/EF-14; весь MVP этой приёмкой не объявляется готовым.

## Авторитетные данные

Layout 3 использует `configuration.yaml`, отдельные descriptors форм/макетов/
команд/подсистем и явные пути модулей/ресурсов. Descriptor schema 2 содержит
единственный состав `children` и именованные значения `properties`.
Native slots хранят порядок/bindings без второй редактируемой копии значения.
`source.json` содержит обязательное происхождение и обратную карту native →
physical source; текущие значения и новые inline members читаются из YAML.
Hashes вручную не обновляются. Lookup/semantic index — производные данные.
Права определяются independently pinned accepted base, а не рабочей картой.

[Схема](readable-layout.md), [descriptors](descriptor-layout.md),
[ресурсы](resource-layout.md), [источники координат](source-map.md).
Открытие прежнего family/readable проекта не меняет его байты и не типизирует
заново preserved значения.

## Редактируемое и сохраняемое покрытие

| Область | Принятое поведение | Граница |
|---|---|---|
| Скаляры/локализованные строки | [32 свойства по owner-specific матрице](typed-scalars.md), без defaults и потери точных целых | Остальные свойства preserved; связи флагов/размеров проверяет платформа |
| Type/TypeSet | [Alternatives/qualifiers](type-description.md), [native targets](type-targets.md), [ограничения именованных наборов](named-type-sets.md) | Непринятый XML shape не угадывается |
| Ссылки/списки | [13 имён/классов/целей](metadata-reference.md), [очистка списка](reference-clear.md), [Enum FillValue](enum-fill-value.md) | Остальные selectors/FillValue остаются preserved |
| Порядок детей | [Перестановка прежних уникальных членов](child-order.md) с native class каждой позиции | Не подмена состава/identity; opaque child сохраняет прежний порядок |
| EnumValue | [Create](enum-value-creation.md), [delete](enum-value-deletion.md), [rename](enum-value-rename.md) | Только известный inline профиль и actual Enum owner |
| Attribute | [Create у Catalog/Document](attribute-creation.md), [create колонок](column-creation.md), [delete](attribute-deletion.md), [rename](attribute-rename.md) | Known field profile и actual root/direct table parent |
| TabularSection | [Create](table-creation.md), [delete](table-deletion.md), [rename](table-rename.md); [четыре service UUID](tabular-section-identities.md) | Только Catalog/Document, известные columns/properties/LineNumber; generated names выводятся из Name |
| Другие metadata classes/sections | Обратимая запись, declared identity/order и прежние доступные typed свойства | Непринятые create/delete/rename/move, новые descriptors и подмена UUID исключаются явно |
| BSL/ресурсы | Собственные файлы, прежние BOM/EOL/байты и [защищённый выбор](protected-export.md) по владельцу | Supplier/binary modules/непроверенное владение восстанавливаются; opaque XML не становится typed form |
| Формы/СКД | Descriptor отделён; native payload сохраняется отдельным ресурсом | Typed elements/layout — EF-12; самостоятельный SDBL — EF-23; RLS/выражения — EF-26 после MVP |

Матрица описывает принятые inverse rules. Сохранённый unknown fragment и успешно
построенный XML envelope не означают разрешение редактировать его внутреннюю
семантику. При неизвестном structural profile strict отказывает до generation,
recovery возвращает принятый состав с диагностикой, сохраняя независимые правки.
Автоматический перенос между родителями и переписывание BSL/форм не приняты.

## Сверка DoD

| Критерий EF-24 | Подтверждение |
|---|---|
| Версии, naming, коллизии и editable/preserved matrix | Schemas/критерии выше; tests generated YAML/resource collision, case aliases, symlinks и namespace conflicts |
| Локальность изменений и самостоятельные descriptors | Descriptor/resource/readable layout; scalar/Type/reference/structural edits меняют только выбранные owner YAML/native XML |
| Явный состав, происхождение отдельно от индекса | Declared children/manifest contexts; Type targets и fresh inline UUID берутся из выбранного состава, без поиска новых файлов |
| Expected-byte migration, no-op, повтор и recovery | [Исходники/база](project-migration.md) и [workspace](workspace-migration.md); receipts, exact settings/inputs, independent pins, отказ по чужим/изменённым outputs |
| Неизвестные части, rename/delete dependencies и поддержка | Общие selectors/guards; parent/member supplier/freeze, selected XML/BSL/typed targets, отказ до публикации; conservative limits перечислены в профилях |
| Source map и общий reader/editor/SDK | Actual UUID/schema paths, Accepted/Working exact revisions, saved/overlay All/Summary; отдельные CLI и SDK используют общую библиотеку |
| Малый реальный CF с платформой | Независимые native/control и ESKA routes C1; последние [результаты](table-rename-results.json) подтверждают CF/build/dump на 1С 8.3.27.2325 |

В последнем полном прогоне прошли 16 root/14 SDK/5 adapter gates и две release
сборки. Из него отдельно сверены 15 acceptance tests: коллизии/unknown XML,
separated import, source/project/workspace migration no-op/repeat/interruption/
stale inputs и independent authority, source-map hashes и table rename origins.
Повторный Cargo-прогон ради этой документации не запускался.

C1: 3 595 descriptors/8 346 объектов; оба CF dump совпали по 8 769 файлам/
8 064 каталогам, кроме 7 011 configVersion. Source/base/input/settings/manifest/
pin и версия сохранены. Независимый native control до платформы учитывает только
предусмотренные final-EOL/empty-ChildObjects нормализации; raw platform metadata
XML byte no-op не заявляется. Остальные результаты частей EF-24 сохраняют
собственные ограничения; исторический список ожидаемых профилей не заменяет
эту итоговую матрицу.

EF-24 — `DONE` в указанном профиле. EF-12 — следующая обязательная задача:
сначала реальная матрица managed forms/ID/references, затем elements/layout,
валидация, writer/reader/source map и платформенная приёмка. Обычные двоичные
формы сохраняются ресурсами. CFE/EPF/ERF, new/init, повторный импорт, analyzer,
Explorer/установка и приёмка четырёх артефактов остаются в текущей очереди MVP.
