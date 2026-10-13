# Создание реквизитов справочника и документа

Двадцатая часть EF-24 разрешает новые inline `Attribute` у принятого `Catalog`
или `Document`. Используются общий protected selector и прежние `prepare`/`build`;
новых команд или параметров нет. Поддержаны обе descriptor schemas и layouts 1/2/3.

Минимальный readable элемент `children`:

```yaml
- kind: inline
  object:
    kind: Attribute
    uuid: 60000000-0000-4000-8000-000000000001
    properties:
      Name: НовоеПоле
      Synonym: {state: empty}
      Comment: {state: empty}
      Type:
        alternatives:
        - {kind: type, name: 'xs:boolean'}
    children: []
    native:
      namespaces: []
      attributes: []
      sections:
      - kind: properties
        items:
        - Name
        - Synonym
        - Comment
        - name: Type
          layout:
            prefix: v8
            parts: [{kind: alternative, index: 0}]
            string_fields: []
            number_fields: []
            date_fields: []
```

`v8` и `xs` должны быть объявлены в принятом namespace context владельца с
соответствующими URI. Пример не добавляет namespace и не меняет принятую базу.
В family envelope используются те же значения через `PropertyValue::TypeDescription`.
Все type alternatives/qualifiers имеют одну рабочую копию; slots содержат только
порядок и привязки, как в [описании Type](type-description.md).

## Граница разрешения

Авторитет — independently supplied pin, принятые UUID/class/native descriptor
владельца и прежняя supplier/freeze policy. `permission(owner, Create)` теперь
разрешает допустимые `Catalog`/`Document`; это необходимое условие, а selector
проверяет профиль и итоговый состав. Изменение рабочего class не позволяет
создать EnumValue вместо Attribute или перенести чужую принятую identity.
Permission нового UUID остаётся `unknown_object` до отдельного принятия базы.

Новый реквизит имеет ровно `Name`, `Synonym`, `Comment`, `Type` в этом порядке,
без attributes, local namespaces, children, InternalInfo или preserved XML.
Synonym — Empty/Localized, Comment — Empty/Text. Type — непустой typed description:
новое создание принимает `Type` для XML Schema boolean/string/decimal/dateTime
и Type/TypeSet именованных типов из current-config namespace. Остальные namespaces,
примитивы и пустой Type требуют своего принятого профиля. Все QName/slots проходят
общую проверку; именованные цели проверяются в окончательном declared составе.
Совместимость qualifiers/типов и полные ограничения платформы проверяет 1С.
Профиль Type существующего принятого реквизита этой частью не сужается.

UUID должен быть новым в принятом проекте и единственным в рабочем составе.
`Name` проходит прежнюю Unicode identifier проверку. Имена реквизитов и табличных
частей одного владельца не могут совпадать после `to_lowercase`; формы остаются
в отдельном namespace. Полные ограничения имени/длины и все стандартные имена
платформы эта лексическая проверка не заменяет.

Прежние члены сохраняются ровно один раз, в прежнем относительном порядке.
Новые реквизиты вставляются в группу Attribute перед табличными частями/формами
и остальными native классами. Каждый сдвигаемый принятый член требует собственного
Reorder permission. Stationary freeze допускает append; сдвиг frozen/locked члена
исключает вставку целиком. Preserved child сохраняет весь принятый состав.

Запрещённая вставка использует прежний `{"kind":"create"}` с UUID действительного
владельца или запрещённого сдвигаемого члена. Strict отказывает до публикации;
`--allow-exclusions` исключает новые поля, сохраняя независимые допустимые свойства.
Некорректный разрешённый профиль/имя/тип или коллизия отказывают до публикации,
включая `--allow-exclusions`. Новая вставка вместе с удалением/перестановкой
прежних членов не принимается. Удаление/переименование Attribute, остальные
владельцы и новые standalone descriptors не входят в этот профиль.
[Удаление реквизитов/колонок](attribute-deletion.md) принято отдельным профилем;
[переименование Attribute](attribute-rename.md) также принято отдельно.
[Создание колонок](column-creation.md)
существующей табличной части принято отдельным профилем.
[Новая табличная часть с колонками](table-creation.md) принята следующим профилем.

## Чтение и координаты

Saved/overlay `All`/`Summary` читают новый UUID, свойства и Type непосредственно
из YAML. Native generation сохраняет input владельца и `uses_working_values`.
Координаты нового объекта/Name/Type относятся к Working; mapper требует checksum
точного рабочего текста и равенство рабочего и выбранного native узла. Отсутствие
или изменение этого текста даёт отказ, accepted span нового объекта не создаётся.

Рабочие YAML/BSL, `source.json`, hashes, settings, accepted storage и CI pin не
переписываются. Schema/rules/tool protocol versions сохранены; экспериментальный
permission API расширил существующий Create. Отдельной CST-команды создания нет;
новый узел редактируется вручную до принятия его UUID. Автоматического переписывания
BSL и форм нет.

## Проверки

Сначала установленная 1С 8.3.27.2325 приняла восемь минимальных реквизитов двух
владельцев с Boolean/String/Number/Date и дополнила стандартные свойства при
выгрузке. Это отдельная проверка native профиля до разрешения новых YAML-узлов.

Прошли 16 root gates и оба release build: fmt/check/strict Clippy/default и
all-features tests, Rustdoc и minimal core/project-cli/build-cli boundaries.
Root — 525 unit + 564 integration, 24 opt-in пропущены; minimal project CLI —
332 unit + 31 CLI. Добавлены десять core, один supplier integration и два CLI tests.
Проверены оба envelope, пустой состав, Type targets, профиль/имена, supplier/freeze,
сдвиг членов, неизвестные slots, смешанные правки, source locations, saved/overlay,
сохранность и повторное поколение. SDK — 359 tests и 14 gates; adapter — 70 unit +
12 integration и пять gates.

Реальная C1: двенадцать реквизитов в двух YAML (Boolean/String/Number/Date,
EnumRef и String/Number composite) совпали с независимым native контролем по всем
8 778 файлам/8 077 каталогам, без exclusions. Existing freeze допускал append;
его сдвиг отказал в ru/en до generation, восстановление сохранило новые поля
другого владельца. Release CLI работал с PATH без платформы/Git/оболочки и без
machine config. Все 3 595 readable YAML прошли документационные JSON schemas;
3 595 descriptors/8 342 объектов совпали в saved/overlay All/Summary.

Обе CF собраны и выгружены через 1С 8.3.27.2325: 8 769 файлов/8 064 каталога
равны кроме 7 011 configVersion. Отдельно проверено сохранение всех новых UUID,
имён, type alternatives/qualifiers и materialization стандартных свойств на обеих
routes. Artifact checksums верны; версия проекта, settings, sources, accepted
storage и исходный native input сохранены. [Обезличенные результаты](attribute-creation-results.json).

EF-24 остаётся `IN_PROGRESS`: другие structural/compound profiles ещё нужны.
Клиент, обновление рабочей ИБ, полный BSL analysis и новые CFE/EPF/ERF этой частью
не приняты.
