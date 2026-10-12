# Описание типов, седьмая часть EF-24

`Type` добавлен в общую lossless модель, native writer, semantic projection,
CST editor, protected export и source map. Descriptor envelopes 1/2, layout 3,
rules/profile 1 и default CLI сохраняются. Старое preserved-свойство не становится
typed при открытии или миграции: новая классификация применяется к свежему XML.

## Значения и native bindings

Обычная часть нового descriptor содержит один редактируемый набор значений:

```yaml
properties:
  Name: "Amount"
  Type:
    alternatives:
      - kind: type
        name: "xs:decimal"
    number:
      digits: "15"
      fraction_digits: "2"
      allowed_sign: Any
```

`alternatives` сохраняет порядок и повторы. `type` и `type_set` различаются;
QName spelling хранится один раз и разрешается по namespace context владельца.
Префиксы не определяют смысл сами по себе. Unicode local names проверяются по
[XML 1.0 Fifth Edition](https://www.w3.org/TR/REC-xml/#NT-NameStartChar).

Квалификаторы: `string.length/allowed_length`,
`number.digits/fraction_digits/allowed_sign`, `date.fractions`.
Цифры остаются точными строками, включая ведущие нули и произвольную длину.
Enums сохраняют native spelling: `Variable/Fixed`, `Any/Nonnegative`,
`Date/Time/DateTime`. Пропущенное поле не получает default; null, числовой YAML
token вместо exact digits и неизвестный enum отказываются.

Служебный property slot содержит `{name: Type, layout: ...}`. Layout хранит
native prefix, чередование alternatives/qualifier groups, индексы и порядок полей.
Значений там нет. Каждый индекс и каждое присутствующее поле связаны ровно один
раз; лишние, отсутствующие и повторные slots запрещены. В legacy envelope
`state: type_description` содержит те же `data/native` единственной модели.
[Schema](type-description.schema.json) дополняется runtime-проверкой bindings
и namespace context; JSON Schema сама не проверяет ссылки индексов.

## Матрица и границы

Составной домен принимается у `Attribute`, `CommonAttribute`, `Constant`,
`DefinedType`, `Dimension`, `Resource`, `ChartOfCharacteristicTypes`,
`FilterCriterion`, `SessionParameter`. `StyleItem.Type` имеет другую семантику
и остаётся preserved. Значения Empty/Nil и отсутствие свойства сохраняются
отдельно от описания; частичные непустые qualifiers не дополняются defaults.

Native attributes, локальные namespace declarations, смешанные core prefixes,
неизвестные children/qualifiers/enums, comments/PI/mixed text и неразрешимые QName
сохраняют **весь** `Type` как XML-фрагмент. Частичная типизация не выбрасывает
непонятный остаток. Native writer восстанавливает порядок по slots, semantic
reader строит прежние `Record`/`QualifiedText` непосредственно из общей модели.
XML не становится промежуточной моделью для typed-потребителей.

CST edits меняют существующие alternative names/kinds и qualifier leaves,
сохраняя состав, slots, quoting, comments и transport. Добавление/удаление
alternative или qualifier field, смена native layout и переход между описанием
и Empty/Nil этим editor не поддержаны. Аннотированная миграция связывает поля
`data` и `native` с их отдельными новыми адресами; она не перемещает целые
sequences через CST factory, теряющую последний comment token.

Protected export принимает data по прежним правам UUID, выбирает native bindings
из independently pinned base, восстанавливает запрещённые raw-правки и отказывает
в strict при exclusions. Правка служебного layout не даёт разрешений. Source map
показывает рабочий Type data subtree для изменённого значения; старый preserved
Type сохраняет адрес исходного XML-фрагмента.

Это проверка представления и обратимости. Проверка существования всех metadata
references, platform limits и зависимостей между qualifiers, structural edits,
другие составные свойства и новая публичная orchestration остаются в EF-24.

## Приёмка

Полные Rust gates и minimal/project-cli/build-cli прошли: 419 unit + 544
integration, 22 opt-in пропущены обычным запуском. SDK all-features — 359 тестов;
адаптер BSL — 70 unit + 12 integration. Проверены обе оболочки, точные цифры,
alternative/qualifier order, Unicode/aliases, неизвестные формы, null, unbound
slots, аннотации миграции, CST no-op, native restrictions/freeze и source origins.
Адаптер defined types/registers проверяет saved/draft/restore в обеих оболочках;
его fixtures больше не ищут XML-текст внутри typed YAML.

Свежая C1 сохранила все native bytes и семантику `All`: 3 595 descriptors,
8 330 объектов. В объявленных metadata nodes найдено 3 738 описаний, 13 Empty
и 100 preserved Type. Три разрешённые правки — precision числа, длина строки,
fraction даты — изменили три YAML и три XML, сохранили остальные байты, прошли
no-op и strict protected export без exclusions. Контроль изменяет XML text nodes
непосредственно, независимо от typed writer.
В полном физическом XML найдено ещё четыре Type в необъявленных payload files;
они сохранены без активации объектов. Состав выбирается по declarations и карте,
а не сканированием всех XML в каталоге.

CF контроля и ESKA собраны и выгружены на установленной 1С 8.3.27.2325:
8 769 файлов/8 064 каталога совпали, кроме 7 011 `ConfigDumpInfo.configVersion`;
все остальные байты равны. Checksums артефактов проверены. Исходный XML,
рабочие inputs платформенного цикла, accepted base, settings и версия сохранены.
Это не проверка бинарного равенства CF, client execution, полного BSL analysis,
обновления ИБ, новых профилей CFE/EPF/ERF или больших C2/C3.

[Обезличенные результаты](type-description-results.json).

[Девятая часть EF-24](type-targets.md) добавляет проверку существования
именованных Type/TypeSet по объявленным native GeneratedType и trusted composition.
Совместимость типов и зависимых форм остаётся отдельным ограничением.
