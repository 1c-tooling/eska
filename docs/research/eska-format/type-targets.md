# Цели Type и TypeSet, девятая часть EF-24

Общая проверка состава теперь разрешает именованные альтернативы
[описания Type](type-description.md) по объявлениям `InternalInfo/GeneratedType`.
Схемы descriptor 1/2, source layout 3, source map 2 и rules 1 не изменены.
Значения остаются в `Type.data.alternatives`; новые служебные копии не создаются.

## Разрешение

QName сначала разрешается в действительном namespace context владельца.
Префикс `cfg` не имеет специального значения: alias того же URI работает,
одноимённый префикс другого URI не обозначает тип текущей конфигурации.
Для URI `http://v8.1c.ru/8.1/data/enterprise/current-config` имя с точкой
должно точно совпасть с объявленным `GeneratedType.name`.

Так проверяются `CatalogRef.Sample`, `DefinedType.Value`, `Characteristic.Properties`
и другие именованные типы, фактически объявленные платформой. Проверка не строит
имя непокрытого типа из класса объекта или имени каталога. Bare types и общие TypeSet
(`AnyRef`, `CatalogRef` и другие) не обозначают один объект; поиск конкретной
цели к ним не применяется. Иные namespaces сохраняют прежнюю QName-валидацию.

Объявления читаются только из секции `InternalInfo` в metadata namespace,
а `GeneratedType` — только в URI `http://v8.1c.ru/8.3/xcf/readable`.
Контекст namespace и все native bytes сохраняются. Повторяющиеся объявления
имён неоднозначны; отсутствующее либо некорректное имя известного объявления
отказывает. Другие native sections и undeclared descriptors не создают целей.

[Типизированный профиль табличной части](tabular-section-identities.md)
добавляет обратимую модель двух наблюдаемых деклараций Catalog/Document.
Четыре service UUID сохранены явно; имена table/row выводятся из единственных
Name реального родителя и табличной части. Профиль проверяется при native import
и в общей модели; неизвестный или прежний preserved профиль остаётся opaque.
Это не разрешает угадывать остальные generated types.

Lookup строится на время операции из проверенного дерева ChildObjects вместе
с проверкой [ссылок метаданных](metadata-reference.md). Он не сохраняется в
проект и не заменяет исходники или обязательную обратную карту.
Отсутствующая именованная цель даёт `source_type_target_missing` с UUID,
именем свойства и индексом alternative; тексты XML в ошибку не копируются.

## Редактирование и защита

CST preview проверяет изменённые Type values по independently verified accepted
composition и её namespace scopes. Рабочая подмена namespace не позволяет
обойти проверку. Native slots, quoting/comments и transport меняются по прежним
правилам; equal-value edit сохраняет exact bytes.

Protected export проверяет выбранный состав после восстановления forbidden raw
edits. Запрещённая dangling alternative восстанавливается; разрешённая даёт
отказ до публикации. Подмена native `GeneratedType` в рабочем YAML не создаёт
новую цель: секция восстанавливается из pinned base. Исходники и принятая база
при отказе сохраняются.

Проверка включена при полном чтении package, принятии/open accepted base,
подготовке миграций и экспорта. Адресный source reader по-прежнему не обещает
полную проверку всех файлов проекта. Legacy preserved Type не продвигается
в typed-значение и не получает заявленной проверки внутренних ссылок.
Ранее допустимые typed-описания с отсутствующими целями теперь отказываются;
доверенные pins сами не переписываются.

## Границы

Девятая часть проверяет существование объявленного типа.
[Десятая часть](named-type-sets.md) дополнительно отказывает для вложенного
именованного набора внутри DefinedType, сохраняя обе native alternative spellings.
Раскрытие DefinedType/Characteristic, зависимости характеристик, platform limits/cross-qualifiers,
неизвестные native domains и structural create/delete/rename ещё не проверяются.
Публичная orchestration проекта остаётся в EF-24; новые C2/C3, CFE/EPF/ERF,
client execution и обновление ИБ этой частью не принимаются.

## Проверки

Прошли 15 root/14 SDK/5 adapter gates: fmt/check/strict Clippy/test/Rustdoc,
minimal core и отдельные CLI. Root — 431 unit + 548 integration, 24 opt-in
пропущены обычным запуском. SDK — 359 тестов, адаптер — 70 + 12.
Проверены aliases/URI, missing/ambiguous targets, undeclared descriptors,
namespace rebinding, обе descriptor schemas, CST styles/CRLF/no-op,
восстановление frozen values и невозможность подменить native объявления.

Свежая C1 сохранила native bytes и `All`-семантику 3 595 descriptors/8 330 объектов.
В объявленном составе найдено 3 507 generated type declarations;
1 129 Type и 114 TypeSet alternatives имеют именованные цели. Две правки
ссылочного типа и числового определяемого типа изменили два YAML/XML,
совпали с независимым native text control, прошли no-op и strict export
без exclusions. Все 3 595 YAML прошли прежние JSON schemas.

Контроль и ESKA собраны в CF и выгружены на 1С 8.3.27.2325: 8 769 файлов/
8 064 каталога равны, кроме 7 011 `ConfigDumpInfo.configVersion`. Проверены
checksums; рабочие inputs платформенного цикла, accepted base, исходный XML,
настройки и версия сохранены.

Отдельный отрицательный native probe заменил числовой определяемый тип на
нечисловой. Платформа отказала при импорте: колонка управляемой формы с итогом
требует агрегируемого типа. Основной цикл использует другую числовую цель.
Таким образом существование объявления не подтверждает совместимость всех
зависимых форм; этот класс проверки ещё не реализован в core.
[Обезличенные результаты](type-targets-results.json).
