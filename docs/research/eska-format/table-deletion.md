# Удаление табличных частей

Двадцать шестая часть EF-24 принимает удаление существующей прямой TabularSection
у Catalog/Document вместе с её колонками и generated declarations. Автор удаляет
один inline child из единственного состава `object.yaml`. Работают прежние
prepare/build, layouts 1/2/3 и descriptor schemas 1/2; новых команд/параметров нет.

## Принятый профиль

Native owner/context/UUID берутся из independently pinned базы. Table содержит
typed TabularSectionIdentities, Properties и ChildObjects в прежнем порядке,
без собственных namespaces/attributes, неизвестных секций и owned files.
Name/Synonym/Comment имеют прежние известные typed формы. Дополнительные
ToolTip/FillChecking/LineNumberLength и Catalog Use сохраняют native значения;
никакие defaults при удалении не выводятся и не дописываются.

Проверены четыре наблюдённых C1 профиля: 87 Catalog/29 Document таблиц с optional
StandardAttributes. Этот preserved домен принимается только для единственного
StandardAttribute `LineNumber` с наблюдённым порядком 25 свойств, leaf/nil values
и optional localized Synonym. Неизвестный standard field/property/namespace,
nested content или service profile не даёт права удалить таблицу.
Колонки — только inline Attribute по [общему известному профилю](attribute-deletion.md).
Пустой состав колонок допустим. Новые native imports распознают точные table/row
declarations; прежний opaque InternalInfo при открытии не переклассифицируется.

Рабочий состав должен быть уникальным ordered subset. Creation и перестановка
выживших членов в том же списке не угадываются как deletion. Удаление standalone
descriptors, foreign tables, перенос между owners и автоматическое изменение
ресурсов не входят в этот профиль. При запрещённом переносе strict отказывает;
recovery может принять отдельно разрешённое удаление из исходного owner, сохранив
отказ добавления чужого accepted UUID у другого owner.

## Права и зависимости

Проверяются Reorder родителя, Delete самой таблицы, Delete каждой принятой
колонки и Reorder каждого сдвигаемого выжившего члена. Supplier rules каждого
UUID проверяются отдельно; ESKA freeze наследуется. Stationary frozen siblings
сохраняют прежнее поведение. Четыре service UUID — identities, не отдельные
metadata owners и не источник полномочий. Rust API Delete permission для
TabularSection расширен как необходимое условие; selector дополнительно
проверяет actual parent и полный subtree profile.

Отказ даёт прежнюю operation `{"kind":"delete"}` с UUID реального блокирующего
родителя/таблицы/колонки/сдвигаемого члена. Strict отказывает до generation;
`--allow-exclusions` восстанавливает полный subtree и сохраняет независимые
допустимые свойства и удаления. Unknown profile остаётся отказом операции.

Общий [guard](attribute-deletion.md#зависимости) инвалидирует имя/UUID таблицы,
имена/UUID удалённых колонок и все четыре generated type/value UUID. Проверяются
итоговые выбранные XML/BSL bytes: decoded text/attributes/tags, entity/CDATA,
case-insensitive identifier tokens. Typed table/row Type targets дополнительно
проверяются по окончательному составу. Разрешённое исправление ресурса/Type
участвует в проверке; запрещённая правка сохраняет pinned зависимость.

Оставшаяся зависимость даёт `source_structural_reference_preserved`, неизвестный
typed target — прежнюю ошибку Type index. Неразбираемый XML при invalidation
отказывает. Оба режима заканчиваются до публикации; ConfigDumpInfo остаётся
производным index. Новый XML не содержит удалённых declarations/колонок.

Поиск имён консервативен и может отклонить несвязанное упоминание общего имени.
Binary/non-XML/non-BSL и вычисляемые ссылки не проверяются; автоматического
переписывания обращений и семантического анализа BSL нет.

## Источники и проверка

Source YAML/BSL, manifest/hashes, settings, accepted storage и CI pin не
переписываются. Saved/overlay All/Summary читают текущий состав. Координаты
свойств выжившей таблицы следуют её UUID после удаления предыдущего child;
удалённые identities не имеют native координат в generation.

Десять core, один supplier integration и два CLI tests проверяют оба envelopes,
empty/populated subtree, повтор, сохранность входов, parent/table/column/shifted
и stationary policies, recovery с независимыми правками. Покрыты все service
slots, имена колонок/таблицы, selected XML/BSL, typed row target, unknown native
и StandardAttribute shapes, смешанные составы, saved/overlay и Working spans.
CLI проверяет exit/stdout/stderr, оба языка и одинаковый JSON без новой схемы.

Прошли 16 root/14 SDK/5 adapter gates. Default tests: 583 unit + 569 integration,
24 opt-in пропущены; minimal core — 382, minimal project — 390 unit + 42 CLI;
SDK — 359, adapter — 70 + 12. Schemas/rules/protocol и locale keys прежние.

C1 используется в собственной копии прежнего native dump. Две таблицы,
12 колонок и восемь service UUID удалены в двух YAML адресными CST cuts.
Independent control удаляет только те же полные native subtrees. Все 8 769
файлов/8 064 каталога совпали после двух предусмотренных
[export contract](export.md) нормализаций: final EOL у 3 595 descriptors и empty
ChildObjects spelling у 106 коллекций. Другие исключения не применялись;
побайтовый no-op original platform metadata XML не заявляется.

RU/EN prepare совпали, повтор использовал поколение. Freeze одной колонки
отказал до generation и указал её UUID; recovery восстановил всю её таблицу,
сохранив независимое удаление второй. Source/base/settings/manifest/pin,
original C1 и owned native input сохранены; независимый release CLI работал
без platform/Git/facade в PATH и без machine config.

Все 3 595 readable YAML и пример новой таблицы прошли локальные schemas.
3 595 descriptors/8 332 объектов равны independent control в saved/overlay
All/Summary. Два Comments выживших root полей имеют Working spans, два прежних
Name — Accepted spans с exact revisions/UUID; удалённые таблицы не отображаются
на native координаты. Эти правки выполнены только в отдельной проверочной копии.

Прежняя native C1 таблица со StandardAttribute LineNumber и 14 колонками
проверена на двух копиях: есть 23 BSL и три XML-form упоминания. Оба языка/режима
отказали с `source_structural_reference_preserved`, сохранив source/base/settings
и прежние поколения. Первый отказ возникает в другом metadata XML из-за
консервативного поиска имён; полный проход до BSL/form payload этим сценарием
не подтверждается. BSL/form guard отдельно покрыт unit/CLI fixtures.
BSL не менялся и не анализировался семантически.

Независимые control/ESKA CF собраны и снова выгружены на 1С 8.3.27.2325.
Все 8 769 файлов/8 064 каталога совпали, кроме 7 011 служебных configVersion.
Удалённые table/column/service UUID отсутствуют на обоих маршрутах; остальные
logical identities/порядок/properties/types и все 114 таблиц/456 service UUID
сохранены. Manifest checksums верны; исходники, accepted base, исходный native
XML и версия проекта не изменились.

[Обезличенные результаты](table-deletion-results.json).
EF-24 остаётся `IN_PROGRESS`: остальные structural/compound profiles ещё нужны.
C2/C3, новые CFE/EPF/ERF, клиент и данные рабочей ИБ этой частью не приняты.
