# Переименование реквизитов и колонок

Двадцать пятая часть EF-24 принимает новый Name существующего inline Attribute
у Catalog/Document или их прямой табличной части. Автор меняет один именованный
Name в readable YAML либо его typed Text в legacy envelope. Работают прежние
prepare/build, layouts 1/2/3 и descriptor schemas 1/2; новых команд/параметров нет.

## Профиль и идентичность

Используется тот же [известный native Attribute профиль](attribute-deletion.md):
одна Properties-секция с Name/Synonym/Comment/Type и наблюдёнными дополнительными
native свойствами, без собственных namespaces/attributes, generated identities,
children и owned files. Общий `attribute_profile` имеет двух действительных
потребителей — deletion и rename; старые opaque значения не переклассифицируются.
Неизвестный профиль восстанавливает Name с сохранением независимых допустимых свойств.

Actual parent, class, UUID и namespace context берутся из independently pinned
базы. Поиск рабочего члена идёт по прежнему UUID, не индексу; working kind и
service fields не подменяют права. UUID, позиции, типы, qualifiers и остальные
properties сохраняются, если автор явно не меняет их разрешённым способом.
Пути основного descriptor и resources не зависят от локального имени поля.
Переименование самой таблицы/главного объекта и их generated types не принято.

Проверяются Reorder реального родителя и Rename самого Attribute. Native supplier
rule корня не наследуется автоматически колонкой принятой таблицы; ESKA freeze
наследуется. Stationary frozen siblings не дают права менять себя, но не блокируют
чужой Name без сдвига. `AcceptedBase::permission(uuid, Rename)` для Attribute —
необходимое условие; фактический parent/profile дополнительно проверяет selector.
Обычный `Property(Name)` по-прежнему не разрешает обход structural policy.

Отказ родителя даёт прежнюю operation `{"kind":"rename"}` с UUID родителя;
отказ поля — его UUID. Strict отказывает до generation. `--allow-exclusions`
восстанавливает Name, сохраняя другие независимые разрешённые правки.
Unknown profile не обходится этим параметром. Schemas/rules/protocol versions прежние.

## Имена и зависимости

Имя — непустой Unicode identifier по прежнему validator; коллизии проверяются
после всей выбранной композиции, с `to_lowercase`. Корневые Attribute и
TabularSection имеют общий namespace, колонки — отдельный namespace своей таблицы.
Два UUID могут одновременно обменяться именами, если окончательный состав уникален.
Rename может сочетаться с отдельно разрешёнными create/delete/reorder; каждая
операция сохраняет свои gates. Ошибка имени/коллизия отказывает до публикации
в обоих режимах, не создавая частичную генерацию.

Общий [guard полей](attribute-deletion.md#зависимости) проверяет окончательный
выбранный состав и выбранные XML/BSL байты. При rename прежнее имя становится
invalidated identifier token; UUID остаётся действительной целью. UUID-only
упоминание не отклоняется из-за смены имени. Case-only изменение не инвалидирует
case-insensitive имя и не включает дополнительные payload reads.

Decoded XML text/attributes/tags и BSL body/comments/strings с прежним именем
дают `source_structural_reference_preserved` до generation, включая recovery.
Поиск консервативен: общие несвязанные имена могут дать отказ. Запрещённая рабочая
правка модуля/ресурса восстанавливает pinned байты и не скрывает зависимость;
разрешённое исправление участвует в проверке. Неразбираемый XML при действительной
invalidation отказывает. ConfigDumpInfo остаётся производным index.

Автоматического переписывания форм/BSL и семантического анализа BSL нет.
Binary/non-XML/non-BSL ресурсы и вычисляемые ссылки не проверяются. Сам rename
не подтверждает клиентское выполнение, миграцию данных рабочей ИБ или глобальную
корректность всех ссылок: принимается только описанный профиль и проверенные bytes.

## Источники и координаты

Рабочие YAML/BSL, source manifest/hashes, settings, accepted storage и CI pin
не переписываются. Saved/overlay All/Summary читают текущий Name по тому же UUID.
Working coordinates Name/Comment остаются точными после отдельно разрешённого
reorder/delete соседа; старый индекс списка не становится persistent identity.
Новый accepted UUID или отдельный descriptor при rename не создаётся.

## Проверки

Одиннадцать новых core, один supplier integration и два CLI теста покрывают
оба envelopes: четыре field namespaces, сохранение UUID/типов/порядка, inherited
freeze, actual parent/member diagnostics, stationary siblings и recovery.
Проверены invalid/colliding Names, общий namespace root Attribute/TabularSection,
отдельные колонки, atomic swap и reuse освобождённого имени. Fresh UUID с прежним
именем не позволяет молча перенаправить существующее XML/BSL обращение.

Unknown property/section/namespace/children и другой root class восстанавливаются.
XML/BSL старые имена блокируют prepare; UUID-only reference и case-only spelling
проверены отдельно. Запрещённый working module fix сохраняет pinned зависимость,
разрешённый — участвует в проверке. После reorder/delete соседа Name/Comment
имеют точные Working spans по UUID; saved/overlay All/Summary совпадают.

Прошли 16 root, 14 SDK и пять adapter gates, две release-сборки. Default tests:
573 unit + 568 integration, 24 opt-in пропущены. Minimal core — 372;
minimal project — 380 unit + 40 CLI; SDK — 359; adapter — 70 + 12.
Schemas/rules/protocol версии и locale keys прежние. Изменён экспериментальный
Rust API: Rename permission Attribute расширен как необходимое условие.

C1 используется в собственной копии прежнего native dump. Два root Attribute
и две колонки получили английские/кириллические Names в двух YAML адресными
CST scalar edits, с сохранением соседних bytes и CRLF annotation. Independent
control меняет только Name внутри тех же native Attribute UUID. Все 8 769
файлов/8 064 каталога совпали после двух предусмотренных
[export contract](export.md) нормализаций: final EOL у 3 595 descriptors и empty
ChildObjects spelling у 106 коллекций. Другие исключения не применялись;
побайтовый no-op для original platform metadata XML не заявляется.

RU/EN prepare совпали, повтор использовал поколение. Freeze одного поля отказал
до generation; recovery сохранил три независимых rename и совпал с отдельным
native control. Source/base/settings/manifest/pin, original C1 и owned native
input сохранены; PATH не содержит platform/Git/facade, machine config отсутствует.

Все 3 595 readable YAML и прежний пример Attribute прошли локальные schemas.
3 595 descriptors/8 346 объектов равны independent control в saved/overlay
All/Summary. Четыре новых Name имеют Working spans, четыре прежних Boolean Type —
Accepted spans с теми же UUID и exact revisions; accepted bytes восстановлены
только в отдельный каталог для проверки координат.

Две дополнительные копии C1 переименовывают прежние native поля с реальными
упоминаниями в XML формы и BSL. Оба языка и оба режима отказали с
`source_structural_reference_preserved`, новые поколения не опубликованы,
source/base/settings сохранены. BSL не менялся и не анализировался семантически.

Независимые control/ESKA CF собраны и снова выгружены на 1С 8.3.27.2325.
Все 8 769 файлов/8 064 каталога совпали, кроме 7 011 служебных configVersion.
Четыре новых Name, прежние logical UUID/порядок/properties/types и шесть колонок
каждой выбранной таблицы сохранены на обоих маршрутах. Все 116 таблиц/464 service
UUID сохранили свои объявления. Manifest checksums верны; исходники, accepted
base, исходный native XML и версия проекта не изменились.

[Обезличенные результаты](attribute-rename-results.json).
EF-24 остаётся `IN_PROGRESS`: остальные structural/compound profiles ещё нужны.
C2/C3, новые CFE/EPF/ERF, клиент и данные рабочей ИБ этой частью не приняты.
