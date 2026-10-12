# Ссылки метаданных, восьмая часть EF-24

Общая модель получила 13 новых owner-specific свойств: одиночные ссылки и
упорядоченные списки ссылок. Native writer, semantic reader, CST editor,
protected selector и source map используют те же значения. Descriptor envelopes
1/2, layout 3, rules/profile 1 и default CLI не меняются.

## Представление

```yaml
properties:
  DefaultObjectForm: "Catalog.Goods.Form.Main"
  Owners:
    - "Catalog.Partners"
    - "ChartOfCharacteristicTypes.Properties"
```

Путь чередует точное имя класса и имя объекта. Регистр, Unicode и порядок
сохраняются; путь не заменяется файловым именем или новым ID. Имена проверяются
по XML NCName, классы — по текущему registry. UUID-only references, XML aliases,
пробелы и неразрешимый синтаксис не становятся типизированными ссылками.

Одиночное значение — строка. Список — непустая последовательность строк;
повторы сохраняются. Отсутствие, Empty и Nil остаются отдельными состояниями.
Пустой массив не подменяет `<Owners/>`; null и numeric tokens запрещены.

Native slot списка содержит `{name, references: {item_prefix, xsi_prefix,
type_prefix}}`. Значений там нет. Эти префиксы разрешаются по URI и восстанавливают
`xr:Item xsi:type="xr:MDObjectRef"`. Homogeneous slots не зависят от числа элементов.
Legacy envelope содержит `state: reference/reference_list`; у списка один набор
`items` и отдельный `native`. [Schema](metadata-reference.schema.json) дополняется
runtime-проверками классов, NCName, context, cardinality и целей.

## Матрица

| Свойство | Владелец | Допустимая цель |
|---|---|---|
| `DefaultLanguage`, `DefaultStyle`, `DefaultRoles` | Configuration | Language, Style, список Role соответственно |
| `DefaultObjectForm` | Catalog, ChartOfCharacteristicTypes, Document, ExchangePlan | Form своего владельца |
| `DefaultListForm` | Те же, Enum, InformationRegister, AccumulationRegister, AccountingRegister, CalculationRegister | Form своего владельца |
| `DefaultChoiceForm` | Catalog, ChartOfCharacteristicTypes, Document, ExchangePlan, Enum | Form своего владельца |
| `DefaultFolderForm`, `DefaultFolderChoiceForm` | Catalog, ChartOfCharacteristicTypes | Form своего владельца |
| `DefaultRecordForm` | InformationRegister | Form своего владельца |
| `DefaultForm` | DataProcessor, Report, Constant, FilterCriterion | Form своего владельца либо CommonForm |
| `Owners` | Catalog | Catalog либо ChartOfCharacteristicTypes |
| `RegisterRecords` | Document, Sequence | InformationRegister, AccumulationRegister, AccountingRegister, CalculationRegister |
| `Content` | Subsystem | Объект объявленного дерева |

Одноимённый `Content` у CommonAttribute, FilterCriterion и FunctionalOption имеет
другую форму и этой матрицей не принимается. Другие selectors также остаются
preserved до своих inverse rules. Неизвестная запись, UUID target, атрибут,
локальный xmlns, mixed text, comment/PI, неверный тип или неоднородный prefix
сохраняет **всё** свойство. Поддержка не частична внутри одной XML-коллекции.
Старое preserved-свойство не продвигается при открытии либо миграции оболочки.

## Проверки и правки

Проверка полного пакета строит временный lookup из `ChildObjects`, inline nodes
и объявленных descriptors. Файловое сканирование не добавляет целей. Проверяются
существование, класс, owner формы и неоднозначное имя. Lookup не сохраняется
как второй источник состава. Writer не создаёт XML для проверки ссылок.

CST preview использует independently verified accepted composition как набор
целей, сохраняя owner/UUID выбранного свойства. Export проверяет итоговый состав
**после** восстановления forbidden raw edits. Поэтому запрещённая dangling
reference восстанавливается, а разрешённая dangling reference отказывает до
публикации поколения. Existing source/base/generation не перезаписываются.
Native list prefixes выбираются из pinned base, а не из рабочих служебных полей.

Одиночные ссылки меняются как scalar с сохранением quoting. Существующие элементы
списка меняются по CST; добавление/удаление trailing members непустого block list
использует явный splice строк. Это обходит потерю newline/comment у CST factory.
Изменение длины flow list, multiline member, удаление аннотированного элемента,
переход list ↔ Empty/Nil и правка native bindings этим editor отказываются.
Raw YAML может менять число элементов с сохранением native binding и валидных
целей; добавление/удаление самих metadata objects ещё не реализовано.

Миграция связывает каждый list scalar и native prefix с новым полем, сохраняет
quote styles, comment tokens и однородный CRLF/LF; весь `ObjectSource` проверяется
после переноса. Source map адресует одиночный scalar либо рабочий список целиком.
Semantic projection строит прежние `Text`/`Record`/`TypedText` непосредственно.

В C1 найдены `Owners` с ChartOfCharacteristicTypes и 11 UUID-only references
в Subsystem.Content вне объявленного дерева. Они остаются в preserved lists;
полученный корпус не объявляется полностью валидированным для неизвестных доменов.
Цели Type alternatives, другие selectors/collections, structural operations и
публичная orchestration остаются частью EF-24.

## Проверки

Прошли 15 root/14 SDK/5 adapter gates: fmt/check/strict Clippy/test/Rustdoc,
minimal core и отдельные CLI. Root — 426 unit + 546 integration, 23 opt-in
пропущены обычным запуском. SDK — 359 тестов, адаптер — 70 unit + 12 integration.
Проверены обе descriptor schemas, owner/target validation, unknown XML,
ordered/repeated members, комментарии/кавычки/CRLF, миграция, policy/freeze,
native bindings, отказ до записи и рабочие source coordinates.

Свежая C1 сохранила 3 595 descriptors/8 330 объектов, полную `All`-семантику
и native bytes. Типизировано 472 одиночных ссылки и 123 списка с 2 334 элементами;
девять Subsystem.Content с неизвестными UUID-only ссылками сохранены целиком.
Правка ссылки на собственную форму и перестановка списка изменили два YAML
и два native descriptor, совпали с независимым XML-контролем; no-op и protected
export не дали exclusions. Все 3 595 YAML прошли JSON Schema validation.

Контроль и ESKA собраны в CF и повторно выгружены на 1С 8.3.27.2325.
Совпали 8 769 файлов/8 064 каталога, кроме 7 011 служебных
`ConfigDumpInfo.configVersion`. Проверены checksums артефактов; исходники,
accepted base, настройки и версия проекта сохранились.
[Обезличенные результаты](metadata-reference-results.json).
