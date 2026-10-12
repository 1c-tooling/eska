# Служебные идентификаторы табличной части

Двадцать вторая часть EF-24 типизирует наблюдаемый `InternalInfo` прямой
`TabularSection` справочника или документа. Четыре native UUID сохраняются
один раз; имена generated типов writer получает из единственных `Name`
родителя и табличной части. Общие reader, target lookup, writer и миграция
используют одну модель. Эта часть ещё не разрешает создание табличной части;
[создание с явными UUID](table-creation.md) принято отдельным следующим профилем.

## Единственный источник значения

Readable descriptor хранит служебную секцию отдельно от редактируемых свойств:

```yaml
kind: tabular_section_identities
items:
  prefix: xr
  table:
    type_id: 10000000-0000-4000-8000-000000000001
    value_id: 10000000-0000-4000-8000-000000000002
  row:
    type_id: 10000000-0000-4000-8000-000000000003
    value_id: 10000000-0000-4000-8000-000000000004
```

Секция находится в `object.native.sections` существующей табличной части;
family schema использует `object.sections`. Prefix обязан быть связан
с `http://v8.1c.ru/8.3/xcf/readable` в действительном контексте владельца.
Полная форма items описана [JSON Schema](tabular-section-identities.schema.json).
Форму UUID проверяет schema; parentage, namespace context и коллизии проверяет core.
Generated names и categories не дублируются в YAML:

| Роль | Имя из текущих metadata Name | Native category |
| --- | --- | --- |
| table | `CatalogTabularSection.<owner>.<table>` или `DocumentTabularSection.<owner>.<table>` | `TabularSection` |
| row | `CatalogTabularSectionRow.<owner>.<table>` или `DocumentTabularSectionRow.<owner>.<table>` | `TabularSectionRow` |

Порядок native деклараций — table, row; внутри каждой — TypeId, ValueId.
Writer сохраняет явные UUID, namespace alias, положение секции и BOM/EOL
descriptor. Имена derived типов включены в прежний общий target index;
metadata references и Type/TypeSet используют тот же declared состав.

## Границы и защита

Native import распознаёт только точный обратимый профиль: две декларации
с соответствующими именами/categories, по два различных UUID, известные
элементы/атрибуты и действительный namespace alias. Неизвестные поля, роли,
порядок, comments/CDATA, локальные namespace declarations или иной родитель
остаются preserved XML. Известная секция проверяет четыре UUID против
UUID других modeled objects/служебных ролей того же native descriptor.

Существующий preserved YAML не преобразуется при открытии или миграции.
Это сохраняет прежние pinned bytes и исключает скрытую смену accepted pin.
Новая native выгрузка может получить typed форму только при явном импорте.
Schema/rules/tool protocol versions прежние; добавлены экспериментальные
Rust types `GeneratedIdentity`, `TabularSectionIdentities` и вариант `Section`.
Потребитель с исчерпывающим Rust `match` должен учесть новый вариант.

Служебные UUID не являются metadata object UUID и не дают policy authority.
Protected selector оставляет принятые служебные поля. Raw edits диагностируются
как прежние resource exclusions: strict отказывает до generation,
`--allow-exclusions` восстанавливает UUID и сохраняет независимые разрешённые
свойства. Свободная модель writer выводит имена из текущих Name, но accepted
rename/delete самой табличной части этой частью не разрешены.

Миграция переносит UUID и привязанные annotations/styles в соответствующие
native fields. Она не превращает preserved данные в typed и не принимает
рабочую правку за новую approved base. Новых CLI commands/options, зависимостей
или human strings нет.

## Проверки

До изменения модели проверен минимальный native профиль на установленной
1С 8.3.27.2325. Без `InternalInfo` сборка табличной части отказывает.
Две независимые CF/build/dump операции с четырьмя явными service UUID успешны;
оба набора UUID сохранены и совпали. Инвентаризация C1: 114 табличных частей,
456 service UUID, без коллизий с modeled object UUID.

Девять core tests проверяют оба descriptor envelopes, реальные parent/name
связи, namespace aliases, непокрытые preserved profiles, UUID collisions,
protected recovery, saved/overlay All/Summary и migration annotations/CRLF.
Standalone CLI test проверяет family/readable, ru/en JSON, strict отказ,
восстановление UUID с независимым Comment и сохранность inputs/settings/base.

Прошли 16 root/14 SDK/5 adapter gates и две release-сборки: fmt/check,
strict Clippy, default/all-features tests, Rustdoc и minimal core/project-cli/build-cli
boundaries. Root — 542 unit + 565 integration, 24 opt-in пропущены;
minimal project — 349 unit + 34 CLI. SDK — 359 tests; adapter — 70 + 12.

Реальная C1 в family/readable восстановила оригинальные 8 778 файлов/
8 077 каталогов побайтно. Все 114 табличных частей получили typed профиль
с теми же 456 service UUID; family → readable migration и её repeat сохранили
профиль и native bytes с независимым новым pin. Два разрешённых Comment
изменили два YAML/native descriptors и совпали с независимым текстовым контролем.
Подмена service UUID отказала до generation на обеих локалях; recovery
восстановил UUID и сохранил оба Comment. Все 3 595 readable/2 645 family/
3 595 migrated YAML прошли schemas, пять отрицательных shapes отказаны.
3 595 descriptors/8 330 объектов равны в saved/overlay All/Summary;
новые Comment имеют Working coordinates, native service spans — Accepted
координаты реальных табличных частей. Release project CLI работал с пустым PATH,
без платформы/Git/оболочки и machine config.

Обе CF собраны и повторно выгружены на 1С 8.3.27.2325: 8 769 файлов/
8 064 каталога совпали кроме 7 011 configVersion. На обеих routes отдельно
проверено сохранение исходных 456 service UUID, names/categories и role order
всех 114 табличных частей. Artifact checksums верны; inputs/settings/принятые
базы, source manifest и версия проекта сохранены.
[Обезличенные результаты](tabular-section-identities-results.json).

EF-24 остаётся `IN_PROGRESS`: остальные structural/compound profiles ещё нужны. Клиент, обновление рабочей ИБ,
полный BSL analysis и новые CFE/EPF/ERF этой частью не приняты.
