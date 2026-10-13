# Создание табличных частей справочника и документа

Удаление существующего известного subtree принято отдельно в
[профиле удаления табличных частей](table-deletion.md).

Двадцать третья часть EF-24 разрешает новую inline `TabularSection` у принятого
`Catalog`/`Document`: пустую или с новыми колонками. Работают прежние protected
selector, reader, writer, source map и `prepare`/`build`. Новых commands/options,
зависимостей, human strings и версий schemas/rules/protocol нет.

Минимальный readable элемент `children` с одной Boolean-колонкой:

```yaml
- kind: inline
  object:
    kind: TabularSection
    uuid: 70000000-0000-4000-8000-000000000001
    properties:
      Name: НовыеСтроки
      Synonym: {state: empty}
      Comment: {state: empty}
    children:
    - kind: inline
      object:
        kind: Attribute
        uuid: 60000000-0000-4000-8000-000000000001
        properties:
          Name: Флаг
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
    native:
      namespaces: []
      attributes: []
      sections:
      - kind: tabular_section_identities
        items:
          prefix: xr
          table:
            type_id: 80000000-0000-4000-8000-000000000001
            value_id: 80000000-0000-4000-8000-000000000002
          row:
            type_id: 80000000-0000-4000-8000-000000000003
            value_id: 80000000-0000-4000-8000-000000000004
      - kind: properties
        items: [Name, Synonym, Comment]
      - kind: children
```

`xr`, `v8`, `xs` должны иметь соответствующие native URI в принятом контексте
родителя. Новые local namespaces не разрешены. Family envelope использует ту же
модель через `Section::TabularSectionIdentities`, `Properties`, `Children`.
Пустая таблица содержит `children: []` и сохраняет тот же children slot.

## Авторитет и состав

Новый объект имеет ровно три секции в порядке InternalInfo/Properties/ChildObjects.
Properties — Name/Synonym/Comment; Synonym — Empty/Localized, Comment — Empty/Text.
Все колонки — новый минимальный Attribute с Name/Synonym/Comment/Type по
[принятому профилю](attribute-creation.md). Reference/preserved children, opaque
InternalInfo, дополнительные свойства/sections/attributes требуют своего профиля.
Type каждой колонки проверяется в действительном контексте родителя и конечном
declared составе; отсутствие именованной цели не обходится recovery mode.

Native table/row names выводятся из единственных Name реального принятого
родителя и новой таблицы. Четыре service UUID заданы явно в
[typed identities](tabular-section-identities.md). Они не становятся metadata
object identities и не дают supplier/freeze authority. Логические UUID таблицы
и колонок также остаются `unknown_object` до отдельного принятия новой базы.

Каждый новый metadata/service UUID уникален в итоговом declared составе и не
совпадает с принятыми metadata UUID или известными TypeId/ValueId слотов
GeneratedType. Последние проверяются также в старом preserved InternalInfo:
это чтение известных native UUID slots, без изменения classification/bytes.
Произвольные строки UUID в ресурсах не считаются service identities.

Имена Attribute/TabularSection родителя имеют прежний общий field namespace;
имена колонок проверяются отдельно внутри новой таблицы. Используется прежняя
Unicode identifier проверка и сравнение `to_lowercase`; полные ограничения
платформы и стандартные имена по-прежнему проверяет 1С.

Новый subtree разрешает принятый родитель с Create permission. Существующие дети
сохраняются ровно один раз и в прежнем относительном порядке: Attributes,
TabularSections, затем остальные native классы. Сдвигаемый принятый член требует
Reorder permission. Stationary frozen field допускает append; запрещённый сдвиг
или parent freeze/native restriction исключает вставку целиком.

Strict отказывает до generation; `--allow-exclusions` восстанавливает принятый
состав, сохраняя независимые допустимые правки. Ошибочный разрешённый профиль,
тип, имя или UUID отказывает в обоих режимах до публикации. Accepted class,
UUID и parentage нельзя заменить рабочими service fields. Unknown preserved
child сохраняет весь принятый состав. Создание вместе с удалением/перестановкой
старых детей не разрешает эти операции. [Удаление таблицы](table-deletion.md),
[удаление колонок](attribute-deletion.md) и [их rename](attribute-rename.md)
приняты отдельными профилями. Rename таблицы, прочие владельцы и самостоятельные
новые descriptors ещё требуют своих правил.

## Источники и координаты

Saved/overlay All/Summary читают новые таблицу/колонки/Type напрямую из YAML.
Новый объект, Name и Type имеют Working coordinates с реальными UUID таблицы/колонки.
Native service spans отображаются на Working-узел новой таблицы; отдельный span
YAML-слота UUID не обещается. Mapper учитывает фактическую секцию Properties
после InternalInfo; он требует checksum точного текста и равенство выбранного
рабочего/native узла. Accepted span новой identity не создаётся.

`source.json`, hashes, рабочие YAML/BSL, accepted blobs, настройки и независимо
переданный CI pin не переписываются. Отдельной CST-команды создания нет:
subtree редактируется вручную до принятия его UUID. Открытие/миграция прежнего
preserved InternalInfo не преобразует его в typed.


## Проверки

Установленная 1С 8.3.27.2325 сначала приняла минимальные native табличные части
двух владельцев с четырьмя колонками Boolean/String/Number/Date каждой. Два
независимых CF/build/dump цикла сохранили заданные metadata/service UUID и типы.

Десять core, один supplier integration и два CLI tests проверяют оба envelopes,
пустой/заполненный состав, parentage, профили, имена и class groups, type targets,
коллизии metadata/service UUID между descriptors и в preserved InternalInfo,
unknown child, supplier/freeze/сдвиги, смешанные правки, saved/overlay и координаты.
RU/EN JSON одинаков; CRLF/annotations и входные байты сохранены.

Прошли 16 root/14 SDK/5 adapter gates и две release-сборки: fmt/check,
strict Clippy, default/all-features tests, Rustdoc и minimal feature boundaries.
Root — 552 unit + 566 integration, 24 opt-in пропущены; minimal project —
359 unit + 36 CLI. SDK — 359 tests; adapter — 70 + 12.

В C1 созданы две табличные части/12 колонок/восемь service UUID в двух YAML.
Release source CLI с пустым PATH без платформы/Git/оболочки сохранил inputs,
accepted storage, settings, manifest и pin. XML побайтно совпал с независимым
контролем: 8 778 файлов/8 077 каталогов. Frozen field остался на месте;
parent freeze отказал до generation, recovery сохранил независимую таблицу.
Подмена нового service UUID принятым metadata UUID отказала в обоих режимах.
Все 3 595 YAML и пример прошли schemas; 3 595 descriptors/8 344 объектов совпали
в saved/overlay All/Summary. Проверены десять Working spans новых таблиц,
колонок и service declarations, включая Properties после InternalInfo.

Native control и ESKA собраны в CF и повторно выгружены: 8 769 файлов/
8 064 каталога совпали кроме 7 011 configVersion. Обе routes сохранили новые
metadata/service UUID, имена, Synonym/Comment, типы и значения qualifiers.
Старые 114 typed таблиц/456 service UUID сохранены. Платформа добавила семь
свойств таблице Catalog, шесть — Document и 26 — колонкам; порядок групп
qualifiers композита нормализован платформой, сами значения/alternatives сохранены.
Checksums верны; input/settings/base/source manifest и версия не изменены.
[Обезличенные результаты](table-creation-results.json).

EF-24 остаётся `IN_PROGRESS`: прочие structural/compound profiles ещё нужны.
Клиент/рабочая ИБ, полный BSL analysis и новые CFE/EPF/ERF не приняты.
