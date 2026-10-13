# Удаление реквизитов и колонок

Двадцать четвёртая часть EF-24 разрешает удаление принятого inline `Attribute`
из Catalog/Document или их прямой табличной части. Автор удаляет целый элемент
`children` либо оставляет явный пустой массив колонок. Работают прежние
`prepare`/`build`, layouts 1/2/3 и descriptor schemas 1/2; новых параметров нет.

Удаление полного table subtree принято отдельно в
[профиле табличных частей](table-deletion.md).

## Профиль и поддержка

Состав — уникальный ordered subset: прежние члены сохраняются один раз и в
прежнем относительном порядке. Смешанные create/delete, перенос UUID, reorder,
удаление формы/табличной части или отсутствующий список не дают права удаления.
Preserved child сохраняет весь принятый состав. Actual parent/class/UUID/context
берутся из независимо закреплённой базы; рабочие служебные поля их не заменяют.

Удаляемый Attribute содержит одну Properties-секцию, без namespaces/attributes,
children, generated types и отдельных descriptors. Первые поля — Name/Text,
Synonym/Empty|Localized, Comment/Empty|Text, Type/TypeDescription. Допускаются
наблюдённые native свойства: PasswordMode, Format, EditFormat, ToolTip,
MarkNegatives, Mask, MultiLine, ExtendedEdit, MinValue, MaxValue,
FillFromFillingValue, FillValue, FillChecking, ChoiceFoldersAndItems,
ChoiceParameterLinks, ChoiceParameters, QuickChoice, CreateOnInput, ChoiceForm,
LinkByType, ChoiceHistoryOnInput, Use, Indexing, FullTextSearch, DataHistory.

Удаляется весь узел, поэтому inverse writer для каждого default значения не
нужен. Preserved properties проверяются как одиночные native элементы известных
доменов в действительном namespace context. Неизвестное имя свойства/секция
или неподтверждённая структура восстанавливаются; старые opaque значения не
типизируются при открытии. В C1 у 1 634 реквизитов/колонок наблюдены три порядка
native properties, включая полные 26–29 свойств.

Проверяются Reorder владельца, Delete каждого удаляемого UUID и Reorder каждого
сдвигаемого соседа. Stationary freeze допускает удаление последующих членов.
Supplier rules остаются отдельными для каждого UUID: native lock корня не
наследуется автоматически принятой табличной частью. ESKA freeze наследуется.
`AcceptedBase::permission(uuid, Delete)` расширен для Attribute; это необходимое
условие, а фактический parent/profile дополнительно проверяет общий selector.

Запрет восстанавливает коллекцию целиком с прежней `{"kind":"delete"}`
диагностикой. Strict отказывает до generation; `--allow-exclusions` сохраняет
независимые допустимые правки. Unknown profile не обходится этим параметром.
Schemas/rules/tool protocol и wire kind прежние.

## Зависимости

Общий guard проверяет итоговый выбранный состав и выбранные payload bytes.
Прежние exact UUID/enum canonical keys сохраняются. Для удалённого реквизита
или колонки дополнительно проверяется имя как case-insensitive identifier token:
например, `Объект.Поле`, canonical metadata path, BSL-обращение или строка с именем.
Подстрока внутри другого identifier не считается совпадением.

Preserved XML properties/sections/children, native/transport attributes и XML
payloads проверяются после entity/CDATA decoding; для полей также проверяются
имена XML-элементов. Комментарии/PI XML не имеют авторитета. BSL payloads читаются
только при фактическом удалении поля; поиск консервативен, включая comments/strings,
без заявления о семантическом анализе BSL. Общие имена могут дать отказ даже
при несвязанном упоминании: неизвестная inverse семантика не угадывается.

Совпадение даёт `source_structural_reference_preserved` с native path и UUID;
неразбираемый XML — `source_structural_xml_unverified`. Эти отказы происходят
до публикации в обоих режимах. Запрещённая правка ресурса/модуля восстанавливает
pinned байты и не скрывает зависимость; разрешённое исправление участвует в проверке.
Без фактической structural invalidation дополнительные payloads не читаются.
ConfigDumpInfo остаётся производным service index и исключён из поиска.

Typed references проверяет прежний target index окончательного состава.
Проверка бинарных/non-XML/non-BSL ресурсов и вычисляемых ссылок не заявляется.
Автоматического переписывания форм/BSL, глобального rename или клиентской
приёмки данных рабочей ИБ нет; эти ограничения не заменяются поиском текста.

## Источники и координаты

Рабочие YAML/BSL, source manifest, hashes, settings, accepted blobs и CI pin
не переписываются. Табличная часть сохраняет UUID и InternalInfo после удаления
последней колонки. Saved/overlay All/Summary отражают текущий состав без ghost
members. Разрешённый Comment выжившего поля имеет Working coordinates по его
UUID после сокращения списка. Удалённый узел не получает native координату.

## Проверки

Десять core, один supplier integration и два CLI теста покрывают оба envelopes:
root/table collections, пустые колонки, сохранение UUID/порядка/service bytes,
полные известные native properties и отказ при unknown profile. Проверены
native supplier rules, inherited freeze, deleted/displaced/stationary members,
восстановление коллекции с независимой правкой и оба языка/JSON. Guard проверен
на decoded XML text/attributes/tags, UUID, CDATA/entities, BSL body/strings/comments,
identifier boundaries и выбранных accepted/working bytes. Malformed XML отказывает.

Прошли 16 root, 14 SDK и пять adapter gates, две release-сборки. Default tests:
562 unit + 567 integration, 24 opt-in пропущены. Minimal core — 361;
minimal project — 369 unit + 38 CLI; SDK — 359; adapter — 70 + 12.
Изменений human strings и wire schema нет. Расширен экспериментальный Rust API
Delete permission для Attribute; произвольный working kind права не даёт.

C1 используется из отдельной копии прежней платформенной выгрузки, с двумя
полными реквизитами из ранее принятого native профиля. Входной XML не меняется.
Два root Attribute и две колонки удалены адресными CST cuts в двух YAML;
accepted storage, settings, manifest/hashes, pin и original C1 сохранены.
Сокращение до последней пустой коллекции отдельно проверяют core tests.

Independent control режет те же native UUID непосредственно в XML. Согласно
[контракту экспортёра](export.md), контроль применяет только оформление metadata:
завершающий EOL у 3 595 descriptors и развёрнутый empty ChildObjects у 106
коллекций. После этих двух явных нормализаций все 8 769 файлов/8 064 каталога
совпали с prepare побайтово; другие лексические исключения не применялись.
Это не заявление о побайтовом no-op для исходной платформенной выгрузки.

RU/EN prepare совпали, повтор использовал прежнее поколение. Freeze удаляемого
поля отказал до generation; recovery сохранил три независимых удаления и совпал
с отдельным native control. Все 3 595 readable YAML и прежний пример Attribute
прошли локальные JSON schemas; 3 595 descriptors/8 342 объектов равны контролю
в saved/overlay All/Summary, без удалённых UUID. В отдельной копии две разрешённые
правки Comment выживших колонок получили точные Working spans, их прежние Name —
Accepted spans; source/base основного платформенного сценария сохранены.

Две дополнительные копии C1 удаляют прежние native поля с существующими
упоминаниями в XML формы и BSL. Оба языка и оба режима отказали с
`source_structural_reference_preserved`; новые поколения не опубликованы,
рабочие/accepted/settings bytes сохранены. BSL не менялся и не анализировался
семантически: это проверка консервативного guard на реальном ресурсе.

На 1С 8.3.27.2325 независимые control/ESKA CF собраны и выгружены: все 8 769
файлов/8 064 каталога равны, кроме прежних 7 011 configVersion. Четыре удалённых
UUID отсутствуют; порядок, identities, свойства и типы выживших детей совпали
с native control. Все 116 прямых таблиц Catalog/Document и 464 service UUID
сохранили declarations. Manifest checksums верны, inputs/settings/base и версия
не изменились. [Обезличенные результаты](attribute-deletion-results.json).

EF-24 остаётся `IN_PROGRESS`: другие structural/compound profiles ещё нужны.
[Переименование реквизитов/колонок](attribute-rename.md) принято следующей частью
по тому же native profile; это не смешанный create/delete selector.
C2/C3, новые CFE/EPF/ERF, клиентское выполнение и обновление данных рабочей ИБ
эта часть не принимает.
