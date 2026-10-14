# eska development tracker

Этот каталог — рабочая декомпозиция продуктовой спецификации
[`eska-roadmap.md`](../eska-roadmap.md). Исходный документ отвечает на вопрос
«что строим», а этот трекер — «что делать следующим и что уже готово».

## Приоритет и MVP от 2026-10-11

По запросу пользователя ближайшая цель — работа с боевыми проектами в новой
структуре ESKA: new/init, импорт изменений из CF/CFE/EPF/ERF в существующий
проект, сборка, дерево Explorer с прежней информацией о поддержке, подсветка
и анализ кода. После этого — fmt, затем VCS.

**Текущая задача: EF-25 (`IN_PROGRESS`) — повторный импорт в существующий проект.**
Первая часть: [build core и `eska-build unpack`](../research/eska-format/native-unpack.md)
распаковывают четыре native типа в собственный staging/новый XML-каталог;
[Подтверждённый ancestor и библиотечный план](../research/eska-format/import-ancestry.md)
сохраняют независимые правки под старым CI pin.
[Применение и recovery выбранного проекта/member](../research/eska-format/source-import-recovery.md)
сохраняют old source, raw ancestry и точные settings в журнале.
[Связка native staging с project plan](../research/eska-format/native-project-import.md)
прошла artifact/import/build/dump четырёх типов, сохранила local BSL и pinned
bookkeeping. [Слияние descriptor values](../research/eska-format/import-descriptors.md)
сохраняет local properties/annotations при incoming inline changes; четыре
native цикла и новый default реквизит CF прошли проверку под прежним pin.
[Native repeat](../research/eska-format/import-repeat.md) четырёх типов сохраняет
bytes/mtime и history; controlled CDF variation сохраняет confirmed ancestry.
Новая карта top-level объектов, native CLI lifecycle и координация IDE ещё в работе.
EF-25 не закрыта.
[Удаление owner](../research/eska-format/import-deletion.md) под прежним pin сохраняет независимые
local values и блокирует dirty/frozen/reference conflicts, включая frozen потомка.
Четыре native пары прошли import/build/dump; CLI/IDE остаются в EF-25.
[Новая карта rename](../research/eska-format/import-composition.md) сохраняет
UUID, local bytes и прежний pin при переносе unrestricted owner. Protected build
и source map 6 проверяют изменённые привязки.
[Добавление owner](../research/eska-format/import-addition.md) проверяет прежнего
родителя/fresh identities под тем же pin. Source map 7 хранит working-only
origins новых metadata/forms, включая общий family YAML; формы/queries/ресурсы
прошли четыре native import/build/dump и 22 Rust gates. Native CLI/IDE ещё в работе.
EF-15 — `DONE` в [принятой матрице](../research/eska-format/base-update.md).
Первый [библиотечный план](../research/eska-format/base-update.md) разделяет
старую базу, независимо принятую поставку и локальные правки.
Добавлен standalone `eska-project update-base` с preview/write, независимыми
pin и восстановлением частичной публикации. Контролируемая CF-пара 1.0.0 → 1.1.0
прошла перенос модуля по UUID, native запрет, strict prepare и независимые
build/dump на 1С 8.3.27.2325. Whole-workspace update сохраняет settings/наследование,
сопоставляет поставки по имени и требует отдельные pin всех members. Настоящий
workspace из двух CF-проектов прошёл strict prepare/native build; новый запрет
одного member блокирует весь output. Guard/resume и полный набор Rust gates прошли.
Производственная пара и причина первичного file export отказа 1С не проверены.
[EF-14 принята](../research/eska-format/external-projects.md): CFE/EPF/ERF
прошли собственные независимые import/edit/build/dump циклы; непустые ERF,
экспортные версии и порядок инициализации выполнены в клиенте 1С 8.3.27.2325.
Freeze проверен для всех трёх профилей. Анализ и общий new/init/import ещё в работе.
[T84 принята](../research/eska-format/external-version.md): экспортная `Версия()`,
явная миграция, чтение/bump, doctor и CI preflight; обе схемы EPF/ERF прошли
build/dump и выполнение экспортных функций на 1С 8.3.27.2325.
[T83 принята](../research/eska-format/project-version.md): три и четыре компонента,
наследование схемы, совместимый JSON и CF/CFE build/dump на 1С 8.3.27.2325.
[Native bindings и codec](../research/eska-format/native-query-codec.md) проверены
на 823 формах/51 схеме СКД C1: 27 запросов СКД и 123 списка прошли no-op и
text edits/reparse. [Версионированные ссылки](../research/eska-format/query-references.md)
перенесли все 150 values в plain файлы с disk no-op/edit/reparse; missing/extra
values отказаны, source namespace удалён из output.
[Проектная карта 5](../research/eska-format/query-project.md) добавляет opt-in
import/export и адресный reader: 153 query resources и 150 `.sdbl` C1.
[Protected writer и source map 4](../research/eska-format/query-protected.md)
выбирают plain values по принятым Resource правам владельца и возвращают
точные координаты `.sdbl`; missing permitted values отказывают до публикации.
[Точечный editor](../research/eska-format/query-edit.md) сохраняет один plain
файл через общий conflict-checked writer и принимает несохранённый текст.
[Явная миграция](../research/eska-format/query-migration.md) переносит working
source и independently pinned base, standalone проект/workspace в map 5.
[EF-23 принята](../research/eska-format/query-acceptance.md): изменённые условия
СКД/списка прошли независимый CF/build/dump и реальное исполнение на 1С
8.3.27.2325. CLI defaults сохранены; анализ потребителей остаётся EF-27.
[EF-12 принята](../research/eska-format/managed-form-acceptance.md) для C1/logform 2.20:
единый form.yaml, reader/codec/editor, protected writer/source map, явная миграция
и независимый CF/build/dump изменённой формы на 1С 8.3.27.2325.
[EF-24 принята](../research/eska-format/structure-acceptance.md) для configuration/
Designer 2.20: итоговая матрица и DoD подтверждены общими Rust gates и независимым
CF/build/dump C1. Unknown/preserved и непринятые structural profiles сохраняют
явные ограничения. MVP четырёх артефактов ещё не принят. Записи частей EF-24 ниже
содержат исторические статусы и ожидания; текущую границу задаёт итоговая матрица.
EF-12 начала [матрицу native форм](../research/eska-format/managed-form-inventory.md):
823 managed dialogs, отдельные UI/Attribute/Command/column ID scopes и fixed
slots. [Typed identity/layout и адресный reader](../research/eska-format/managed-form-layout.md)
проверены на всех 823 dialogs: scopes, fixed edges, cycles/nesting и известные
references. Native inputs сохранены.
[Обратимый body codec](../research/eska-format/managed-form-body.md) сохраняет
байты 823 форм через YAML и проверяет 665 перестановок после reparse.
[Readable properties](../research/eska-format/managed-form-properties.md)
подтверждены на 12 317 properties C1; body v2 сохраняет чтение v1 и единственные
values. 751 форма прошла codec edits/reparse.
[Единый form.yaml](../research/eska-format/managed-form-project.md) объединяет
metadata descriptor и body v2 через обратную карту версии 4. Все 823 формы
прошли project reader/export; no-op совпал с прежним readable контролем,
правки properties/layout изменили только 799 native dialog XML.
[Protected writer и source map 3](../research/eska-format/managed-form-protected.md)
подключены к map 4: права определяет принятый metadata UUID формы, native bindings
закреплены, разрешённые properties/layout имеют Working координаты. В C1 2 720
values и 169 перестановок выбраны для 183 форм; 637 запрещённых форм восстановлены.
[Точечные edits](../research/eska-format/managed-form-edit.md) проверяют принятую
identity и Resource permission, меняют existing scalars/collections через CST
и используют общий conflict-checked writer.
[Миграция](../research/eska-format/managed-form-migration.md) переносит рабочий
source и независимо pinned base в map 4, сохраняя settings проекта/workspace.
Независимый XML и реальный CF цикл подтвердили изменённые Title/layout:
fresh dumps совпали по 8 769 файлам/8 064 каталогам, кроме служебных configVersion.
CLI defaults сохранены; прежние migration targets сохраняют map 4 как byte no-op.
EF-12 — `DONE` в указанном профиле; MVP четырёх артефактов остаётся незавершённым.

Первая часть расширяет [typed writer](../research/eska-format/typed-scalars.md)
на 28 bool/integer/enum свойств с прежней защитой и точечными правками.
Вторая часть добавляет [самостоятельные descriptors и миграцию исходников](../research/eska-format/descriptor-layout.md)
с обратной картой версии 2 и общим reader/editor/SDK.
[Третья часть](../research/eska-format/accepted-layout.md) переносит trusted base
из pinned blobs с прежними policy/freeze и явным новым pin.
[Четвёртая часть](../research/eska-format/resource-layout.md) отделяет физические
пути модулей/ресурсов от native paths с картой версии 3; общий reader, защита,
source map и SDK используют эти привязки. [Пятая часть](../research/eska-format/readable-layout.md)
добавляет именованные свойства в descriptor schema 2 с отдельными native details,
общими CST/semantic paths, pinned миграцией и поддержкой SDK. Orchestration проекта,
остальные составные домены, ссылки/коллекции и structural operations
ещё нужны; публичный CLI импорта сохраняет прежнее расположение.
[Шестая часть](../research/eska-format/annotation-migration.md) переносит аннотации
legacy YAML с привязкой к исходному полю/объекту, сохраняя значения и CST styles.
[Седьмая часть](../research/eska-format/type-description.md) добавляет Type:
alternatives/qualifiers, единственные editable values, native bindings,
точечные edits и защиту с прежними source origins. Три реальные правки C1
собраны в CF и сверены с независимым XML-контролем на 1С 8.3.27.2325.
[Восьмая часть](../research/eska-format/metadata-reference.md) добавляет 13 имён
ссылок/списков и проверку целей по объявленному дереву после восстановления
запрещённых правок. Два YAML/XML изменения C1 прошли CF/build/dump с тем же
независимым контролем; остальные составные домены и structural operations нужны.
[Девятая часть](../research/eska-format/type-targets.md) разрешает именованные
Type/TypeSet по native GeneratedType из trusted composition, сохраняя namespace
scopes и прежние схемы. Две правки C1 прошли CF/build/dump. Отрицательный probe
подтвердил, что существование типа не заменяет проверку зависимых форм.
[Десятая часть](../research/eska-format/named-type-sets.md) запрещает вложенные
DefinedType/Characteristic внутри DefinedType по native category. Обе XML-записи
alternative сохраняются: платформа принимает и нормализует их. Проверки root,
SDK и адаптера прошли; ограничения подтверждены независимыми native probes.
[Одиннадцатая часть](../research/eska-format/readable-project-import.md) добавляет
`eska-project import --layout readable`, сохраняя default family, freeze и
самостоятельный pin. Независимый бинарник восстановил C1 побайтно без платформы
и Git; обе локали и полные Rust gates прошли. Миграция через CLI и structural
operations остаются в EF-24.
[Двенадцатая часть](../research/eska-format/project-migration.md) связывает оба
плана в `eska-project migrate`: preview и явная публикация readable standalone
копии, exact settings, независимый новый pin, repeat и process recovery.
Workspace inheritance отклоняется явно; исходный проект и CI approval сохраняются.
Полные Rust gates прошли; C1 принята через CF/build/dump на 1С 8.3.27.2325
с независимым native контролем. Структурные операции и прочие составные домены
остаются в незавершённой EF-24.
[Тринадцатая часть](../research/eska-format/child-order.md) добавляет перестановки
существующих детей по родным UUID/декларациям при прежнем составе и native class
каждой позиции. Проверяются владелец и перемещённые элементы, stationary freeze
сохраняется; запрещённый порядок восстанавливается без потери независимых свойств.
Saved/unsaved корень принимает прежние декларации в новом порядке.
Create/delete/rename, прочие составные домены и workspace migration ещё нужны.
[Четырнадцатая часть](../research/eska-format/enum-value-creation.md) разрешает
создание inline значений перечисления через принятый UUID/class владельца.
Новые UUID/имена проверяются; сдвиг прежних членов сохраняет policy/freeze.
Working source locations требуют точного совпадения нового YAML/native узла.
Три значения C1 в двух YAML прошли независимый контроль и CF/build/dump на 1С
8.3.27.2325. Остальные creation profiles, delete/rename, составные домены и
workspace migration остаются в EF-24.
[Пятнадцатая часть](../research/eska-format/enum-value-deletion.md) разрешает
удаление принятого EnumValue с проверкой владельца, удаляемых/сдвигаемых членов
и зависимостей выбранного состава. Typed-ссылки и упоминания в decoded preserved
XML/ресурсах отказывают до публикации; запрещённая правка ресурса не скрывает
зависимость. Rename, остальные structural profiles, составные домены и workspace
migration остаются в EF-24.
[Шестнадцатая часть](../research/eska-format/reference-clear.md) разрешает
очистку принятого списка ссылок до `Empty` через общий editor/protected selector.
Прежние readable native bindings можно оставить неизменными; их правки продолжают
диагностироваться. Это позволяет удалить последнюю зависимость до structural edits.
[Семнадцатая часть](../research/eska-format/enum-value-rename.md) разрешает Name
принятого EnumValue с прежним UUID, проверкой namespace/value policy, конечных
коллизий и зависимостей. Старое имя в существующем FillValue отказывает до
публикации; три разрешённые правки C1 прошли независимый CF/build/dump контроль.
Другие structural profiles, составные домены и workspace migration ещё нужны.
[Восемнадцатая часть](../research/eska-format/enum-fill-value.md) принимает
именованный EnumValue в FillValue с отдельным QName binding и проверкой цели
окончательного состава. Reader/editor/source map и миграция аннотаций общие;
старые preserved поля не типизируются при открытии. Две правки C1 прошли
независимый native контроль и CF/build/dump на 1С 8.3.27.2325.
[Девятнадцатая часть](../research/eska-format/workspace-migration.md) добавляет
миграцию всего workspace с exact root/member settings, прежним наследованием
и отдельными approved pins. До записи проверяются все members; outer receipt
блокирует частичную публикацию. Mixed family/readable workspace двух CF members
на C1 прошёл independent native/semantic/authority контроль и два CF round trips
на 1С 8.3.27.2325. Другие structural/compound profiles ещё нужны; EF-24 незавершена.
[Двадцатая часть](../research/eska-format/attribute-creation.md) принимает новые
Attribute у Catalog/Document по минимальному Name/Synonym/Comment/Type профилю.
Права принятого владельца, свежие UUID, имена, Type targets и сдвигаемые члены
проверяет общий selector; schemas/rules/protocol прежние.
Двенадцать полей/два YAML C1 прошли независимый byte/semantic/schema контроль
и CF/build/dump на 1С 8.3.27.2325 с сохранением новых UUID/типов, inputs и версии.
[Двадцать первая часть](../research/eska-format/column-creation.md) создаёт
колонки принятой табличной части Catalog/Document с её UUID, parentage,
namespace context и прежней supplier/freeze policy.
Двенадцать колонок C1 прошли независимый native/schema/semantic контроль и два
CF/build/dump цикла на 1С 8.3.27.2325; UUID/типы, generated identities и inputs сохранены.
[Двадцать вторая часть](../research/eska-format/tabular-section-identities.md)
типизирует четыре service UUID прямой табличной части Catalog/Document;
generated names выводятся из единственных metadata Name. Preserved YAML
не преобразуется при открытии/миграции; UUID не дают policy authority.
[Двадцать третья часть](../research/eska-format/table-creation.md) создаёт
пустые или заполненные табличные части Catalog/Document с явными service UUID,
свежими identity колонок, проверкой принятого родителя, имён/типов и сдвигов.
[Двадцать четвёртая часть](../research/eska-format/attribute-deletion.md) удаляет
реквизиты/колонки Catalog/Document по известному native профилю, с сохранением
порядка остальных членов, проверкой supplier/freeze и XML/BSL упоминаний.
[Двадцать пятая часть](../research/eska-format/attribute-rename.md) переименовывает
эти Attribute с прежними UUID и known native profile; окончательные имена,
права actual parent/member и XML/BSL зависимости проверяются до generation.
[Двадцать шестая часть](../research/eska-format/table-deletion.md) удаляет известный
полный table subtree Catalog/Document, проверяя каждую колонку и оставшиеся
table/column/service dependencies. Остальные structural/compound profiles остаются в EF-24.
Продолжение EF-18 отложено до завершения блока структуры; незавершённый статус
`IN_PROGRESS` и накопленные результаты сохранены. Ранее завершённые задачи
не открываются заново, но не подтверждают новое расширенное покрытие MVP.
[Порядок](#ближайший-порядок-выполнения), [подробные задачи](15-eska-format-experiments.md)
и [обоснование структуры](../research/eska-format/project-layout.md) согласованы.
Старые указания сохранять очередь и ограничиться CF-пилотом заменены этим решением.

[Двадцать седьмая часть](../research/eska-format/table-rename.md) добавляет Name
принятой Catalog/Document таблицы: generated names выводятся из одного значения,
все UUID сохраняются. Проверяются actual parent/table policy и старые XML/BSL/
typed dependencies, включая reuse имени с новым UUID. Source map связывает
три Name spans с единственным Working scalar; service UUID остаются Accepted.
Полные Rust gates и независимый CF/build/dump контроль C1 прошли на
1С 8.3.27.2325. EF-24 остаётся `IN_PROGRESS`; формы и прочее непринятое покрытие
этой частью не объявлены готовыми.

## Подтверждённые результаты и история решений

Записи ниже описывают проверки на момент выполнения, а не текущий порядок работ.

По запросу от 2026-10-08 сформирована отдельная очередь
[экспериментов с форматом исходников ESKA](15-eska-format-experiments.md):
типизированный YAML, обратный экспорт в Designer XML и соблюдение поддержки.
`EF-01` завершена: покрытие, обезличенный корпус, 48 требований и воспроизводимая
инвентаризация пяти реальных образцов. `EF-02` завершена: внутренний descriptor,
строгий YAML, точные значения и native identity; публичный CLI не менялся.
`EF-03` завершена: полный файловый manifest, импорт объявленного состава C1,
сохранение ресурсов и одинаковый результат повторного импорта.
`EF-04` завершена: полный обратный XML-экспорт C1, одинаковые source-модели
всех активных descriptors, byte-preserved ресурсы и отсутствие повторных записей.
`EF-05` завершена: закреплённые байты/политика, независимая ссылка CI, mixed
разрешения и полное восстановление C1 с CF двух поставщиков.
`EF-06` завершена: защищённые поколения, восстановление запрещённых правок,
strict, сохранность прошлого результата и полный byte-equal экспорт C1.
`EF-07` завершена: точечные CST-правки, сохранение оформления, no-op,
проверка принятой identity/прав и конфликтов записи.
`EF-08` завершена в CF-профиле: контрольные платформенные циклы C1,
ровно две допустимые правки и сохранённая поддержка. Применение к БД и клиент
не приняты; ограничения исходного C1 описаны в отчёте этапа.
`EF-09` завершена: release-замеры C1 и ограниченные импорты C2/C3;
экономия места и ускорение обычного Git не подтвердились. Принято продолжение
с обязательной защитой переноса байтов базы через Git.
`EF-10` завершена: общие source/build библиотеки, явная конфигурация ESKA,
ленивое чтение, protected snapshot/map и самостоятельные project/build CLI.
Полный C1 принят обоими инструментами; сборка выполнена на установленной 1С,
байты исходников/базы сохранены.
`EF-17` завершена: выбран общий host bsl-analyzer с адаптером ESKA;
независимые снимки учитывают unsaved YAML/BSL без XML-экспорта. На C1/C2/C3
проверена ограниченная проекция имён и измерена стоимость полного чтения.
Parser отдельно работает без базы анализа. Следующий этап — EF-18: адресный
версионированный ввод, общий IDE/batch host, отмена и бюджеты памяти.
`EF-18` начата: tracked canonical MDO проверен в отдельной копии ядра;
адресная инвалидация и отсутствие XML-fallback подтверждены. Полный этап
и самостоятельные analyzer/check ещё не готовы.
Общий контроллер saved/overlay добавлен в библиотеку без IDE runtime:
неполнота/ошибки, revisions, локальные лимиты и отмена проверены отдельно
от семантического покрытия ядра BSL.
Добавлены отдельный адаптер типов/моделей и закреплённый workspace трёх
изменённых пакетов ядра. Квартальная/годовая периодичности теперь сохраняются.
Добавлена [общая сессия](../research/eska-format/shared-session.md), которая
соединяет saved/overlay, адаптер и реальные resolver ядра. Неверный текущий
ввод очищает slot, соседний resolver сохраняет memo; полнота чтения отделена
от покрытия главных и вложенных моделей. Добавлены [BSL saved/overlay/close](../research/eska-format/bsl-session.md),
diagnostics/completions и текущий YAML-снимок для дерева/свойств с общим clock.
Выполняющийся запрос имеет отдельный токен кооперативной отмены; он не отменяет
соседний handle и не меняет входы. Лимит текущих текстов не заменяет бюджет RSS;
общий бюджет памяти, полнота анализа и analyzer CLI остаются незавершёнными.
Первый [самостоятельный checker](../research/eska-format/check-cli.md) использует
тот же host с явным syntax-профилем и кооперативным deadline. В C1 проверены
1 975 активных модулей, включая 99 вложенных команд; ещё четыре сохранённых
BSL-ресурса принадлежат необъявленным объектам. Синтаксис совпал с reference;
неполнота моделей метаданных сохранила exit 2. EF-18 остаётся `IN_PROGRESS`.
Добавлена [общая квота исходных байтов YAML/BSL](../research/eska-format/source-budget.md):
замены и отказы обновляют один счётчик, результаты CLI показывают покрытие
и текущие ресурсы. Квота не ограничивает весь heap/RSS; эта часть бюджета
памяти остаётся незавершённой.

Добавлены [наблюдения памяти ядра](../research/eska-format/kernel-memory.md)
без запуска запросов и изменения ревизии. На C1 измерено удержание memo после
освобождения 40 модулей; оценки таблиц отделены от RSS и квоты исходных байтов.
Это вход для политики памяти, а не реализованный общий memory budget.
Добавлен [первый отдельный IDE transport](../research/eska-format/analyzer-metadata.md):
явный handshake, страницы состава, чтение/refresh YAML и проверка свежего
UUID сеанса со строковой ревизией. C1/C2/C3 пройдены без BSL-прохода и платформы.
Подключены [документы редактора](../research/eska-format/analyzer-documents.md),
syntax pull, отмена активной диагностики и [общий server/checker deadline](../research/eska-format/analyzer-deadline.md).
Подключено [стандартное автодополнение](../research/eska-format/analyzer-completion.md)
с теми же current inputs, отменой и deadline. На C1/C2/C3 в RU/EN проверены
изменение/ошибка YAML, смена BSL-типа, восстановление после close и активная отмена.
Добавлен [первый property edit plan](../research/eska-format/analyzer-property-plans.md)
для открытого ready YAML: версия/hash проверяются общей сессией, политика/CST —
общим редактором EF-07. На C1 в RU/EN подтверждены разрешённая правка без записи,
native refusal, no-op и сохранность текущего ввода. Полная повторная проверка базы
занимает несколько секунд; [отмена и deadline](../research/eska-format/analyzer-property-cancellation.md)
этого IO уже подключены. Адресная проверка policy ещё требуется.
Добавлен [переход к текущему объявлению BSL](../research/eska-format/analyzer-definitions.md):
local/cross-file targets привязаны к той же revision и target hash, неполнота
не скрывается пустым успехом. Saved/editor/close пройдены на C1/C2/C3 в RU/EN;
отмена/deadline и ID reuse используют общий контроль.
[Координаты YAML](../research/eska-format/analyzer-metadata-locations.md) уже
доступны по UUID/QName без чтения файла; [BSL-символ коллекции](../research/eska-format/analyzer-metadata-definitions.md)
открывает верхнеуровневую YAML-цель готовой канонической модели.
24 сценария четырёх семейств на C1/C2/C3 в RU/EN прошли, включая editor/invalid/close
и declared shadowing. Добавлены [дочерние BSL-символы](../research/eska-format/analyzer-member-definitions.md):
реквизиты, табличные части/колонки строк и значения перечислений используют тип
receiver и точный parent текущего YAML. На C1/C2/C3 в RU/EN прошли 18 сценариев,
108 подтверждённых definition и 108 locate, включая enum rename и structure refusal.
[Поля записей и измерения отбора](../research/eska-format/analyzer-register-definitions.md)
уже покрыты для четырёх видов регистров. На C1/C2/C3 в RU/EN прошли 16 сценариев:
320 подтверждённых definition и 320 locate; регистров расчёта в корпусах нет,
для них пройдены тестовые примеры. Form data и остальные receiver kinds ещё не покрыты.
Все IDE edits, полная семантика и общий memory budget ещё не готовы; EF-18 не завершена.
[Client file events](../research/eska-format/analyzer-watched-files.md) уже
сбрасывают saved YAML/BSL без I/O и сохраняют открытые черновики. На C1/C2/C3
в RU/EN прошли шесть сценариев на собственных частичных копиях с полным составом:
30 подтверждённых definition и 42 locate.
[Control file events](../research/eska-format/analyzer-project-changes.md) уже
завершают прежний host с `eska/restartRequired`; 42 сценария C1/C2/C3 в RU/EN
проверили все семь control inputs, отказ старой сессии и новый процесс.
[Динамическая регистрация](../research/eska-format/analyzer-watcher-registration.md)
запрашивает у поддерживающего клиента одну подписку на 6–7 control paths.
42 control-event и шесть saved-input сценариев с подтверждённой регистрацией
прошли. [Saved-input подписка](../research/eska-format/analyzer-saved-watcher.md)
добавляет компактные source-relative patterns для объявленных YAML/BSL и binary
modules, без обхода каталогов. Настоящий client/OS watcher, автоматическое
открытие нового состава и остальные условия EF-18 ещё в работе.
[Переходы к пространствам общих модулей](../research/eska-format/analyzer-namespace-definitions.md)
подтверждают current owner YAML без чтения target BSL. На C1/C2/C3 в RU/EN прошли
12 сценариев: 72 namespace definition/locate и 48 BSL shadow/alias definition.
Полная семантика, IDE edits, общий memory budget и клиент ещё в работе.
[Переходы для составного типа](../research/eska-format/analyzer-union-definitions.md)
подтверждают только общее объявление поля; nullable arms пропускаются,
разные owners и неподдержанные варианты не дают произвольной цели.
На C1/C2/C3 в RU/EN прошли шесть сценариев: 48 definition/locate и 48 отказов.
Полнота inference, модель формы и остальные условия EF-18 остаются в работе.
Очередность остальных задач не меняется.
[Процессы, задачи и планы обмена](../research/eska-format/analyzer-process-objects.md)
подключены к общей объектной модели и YAML-навигации. Все 76 добавленных моделей
C1/C2/C3 совпали с native reader; в RU/EN прошли 764 definition/locate и 292 отказа.
Адресные реквизиты задач сохраняют реальные UUID; стандартные неявные поля
не получают выдуманных целей. Остальные модели и условия EF-18 ещё в работе.
[Планы характеристик, счетов и расчёта](../research/eska-format/analyzer-chart-objects.md)
добавлены в ту же проекцию: 39 из 40 новых моделей C1/C2/C3 совпали с native reader,
одна явно не поддержана из-за `cfg:AnyRef`. В RU/EN прошли 284 definition/locate
и 184 отказа. Неявные таблицы не получают выдуманных UUID; реальные признаки
объекта и строки сохраняют отдельные объявления. EF-18 остаётся `IN_PROGRESS`.
[Подробные причины отказа моделей](../research/eska-format/analyzer-adapter-issues.md)
передаются одной DTO в дерево/чтение/уведомления; зафиксированы все 14 вариантов.
Шесть stdio-сценариев RU/EN подтвердили текущие ranges/hash, editor/close и два
реальных `cfg:AnyRef` C1. Пройдены 146 tool tests, 19 tool gates и 15 root gates.
[Диагностика текущих YAML-моделей](../research/eska-format/analyzer-metadata-diagnostics.md)
использует общие typed findings и captured coordinates, отдельно от BSL settings.
155 tool tests, 19 tool gates и 15 root gates прошли; 12 stdio-сценариев RU/EN
подтвердили 144 pulls, editor/close/refresh, unsupported warnings и locale parity.
[Подробный отчёт CI](../research/eska-format/analyzer-checker-metadata.md)
использует тот же metadata query и общий presentation. 160 tool tests и все
обязательные gates прошли; 12 сценариев C1/C2/C3 RU/EN подтвердили точные
findings/coordinates/hash. Полный C1 sweep проверил 1 975 модулей без syntax
findings; неполнота моделей сохранила exit 2. EF-18 остаётся `IN_PROGRESS`.
[Структура текущего BSL](../research/eska-format/analyzer-document-symbols.md)
получается из item/region trees общего ядра. Standard documentSymbol согласует
hierarchy/flat и возвращает текущие ranges/hash без чтения соседей. 173 tool tests,
19 tool gates и 15 root gates прошли; 24 сценария C1/C2/C3 RU/EN подтвердили
144 pulls и 768 узлов. EF-18 остаётся `IN_PROGRESS`.
[Карточка текущего символа](../research/eska-format/analyzer-symbol-hover.md)
подключена к standard hover через typed API общего ядра. IDE переводит заголовки
RU/EN и согласует plaintext/markdown; caller ranges/hash сохраняются при изменении
BSL/YAML. 187 tool tests и все 19 tool/15 root gates прошли; 24 сценария C1/C2/C3
подтвердили 192 hover pulls. Scope пока ограничен resolved symbols и возвращаемым
типом метода; EF-18 остаётся `IN_PROGRESS`.
[Подсказки параметров вызова](../research/eska-format/analyzer-call-signatures.md)
подключены к standard signatureHelp через native call binding общего ядра.
Текущие parameter/docs и выбор аргумента сохранены; согласуются markup/UTF-16
labels/null selection. 203 tool tests и все 19 tool/15 root gates прошли;
24 сценария C1/C2/C3 RU/EN подтвердили 240 pulls без записи оригиналов.
Полная семантика и общий memory budget остаются в работе; EF-18 — `IN_PROGRESS`.
[Использования BSL-символа](../research/eska-format/analyzer-references.md)
подключены к standard references через typed native search, без второго индекса.
Текущие target ranges/hash, includeDeclaration и scope подтверждены;
coverage/caps остаются явными, неполный поиск не доказывает безопасность rename.
217 tool tests и все 19 tool/15 root gates прошли; 24 сценария C1/C2/C3 RU/EN
подтвердили 192 pulls и 160 Locations. Native collection ещё не имеет жёсткого
CPU/RSS budget; EF-18 остаётся `IN_PROGRESS`.
[Планы локального переименования BSL](../research/eska-format/analyzer-local-rename.md)
связывают native scope параметров/locals метода с open version/hash и принятой
module policy. Общие права YAML/BSL и защищённого экспорта учитывают UUID
вложенной команды; рабочий UUID не может снять freeze. Сервер возвращает
versioned edits без записи; capture и неполный поиск дают отказ.
233 tool tests, все 19 tool/15 root gates и root 895/14 прошли. 24 сценария
C1/C2/C3 RU/EN на собственных минимальных композициях подтвердили 360 pulls,
96 планов и 228 правок; реальные исходники сохранены. Native CLI дополнительно
проверил синтаксис шести изменённых копий. Полные модели/семантика, общий memory
budget и Explorer ещё в работе; EF-18 остаётся `IN_PROGRESS`.
[Отчёт памяти сессии](../research/eska-format/analyzer-kernel-memory.md)
передаёт прежние native table estimates через общий сервер с обязательным
текущим snapshot, без queries, lazy I/O или cache trim. Повторяющиеся имена
таблиц сохранены; порядок учитывает все значения. Source quota, неполный heap
estimate и недоступный RSS показаны раздельно. 236 tool tests и все 19 tool/15
root gates прошли. Шесть процессов C1/C2/C3 RU/EN с полным составом и одним
загруженным BSL подтвердили 3006 изменений/поисков и 78 снимков; оригиналы
сохранены. Общий memory budget и остальные условия EF-18 ещё в работе.
[Освобождение прежнего разбора BSL](../research/eska-format/kernel-parse-release.md)
устраняет удержание native hint при release и rejected input. Пятый локальный
patch того же ядра использует общий unreadable setter без нового кеша или
disk fallback. 238 tool tests, все 19 tool/15 root gates и kernel 1102/4 прошли.
Парное сравнение по 128 крупным модулям каждого C1/C2/C3 подтвердило освобождение
источника, сохранность 390 оригиналов и различие OS RSS/Salsa estimates.
Entries включает metadata-only memos; прежняя схема статистики сохраняется.
Общий memory budget и остальные условия EF-18 остаются в работе.
[Пакетная очистка вычисленных текстов](../research/eska-format/kernel-source-sweep.md)
включает source memos в прежний deep LRU trim и восстанавливает interactive
window. Сравнение 128 крупных модулей каждого состава подтвердило совпадение
384 пар syntax/hash и уменьшение source estimates до 6,2/36,1/40,1 MB.
Current inputs и 390 оригиналов сохранены; общий heap/RSS budget ещё в работе.
Все 19 tool/15 root gates, 238 tool tests и kernel 1103/4 прошли;
normal graph 206/214 сохранён, новых dependencies нет.
[Семантический профиль checker](../research/eska-format/semantic-check.md)
сохраняет доступные тела до первого query и выполняет настроенные native rules.
Selected-profile coverage, source quota и неполнота остаются явными;
saved reparse hints очищаются без изменения editor inputs. 246 tool tests,
все 19 tool/15 root gates и kernel 1104/4 прошли. Все 1 975 объявленных модулей
C1 проверены; 26 SelfAssign совпали с установленным анализатором по code/severity/
координатам, JSON RU/EN совпал. Неполные модели сохранили exit 2; исходники
сохранены. В этом checker-срезе LSP pull ещё оставался syntax.
[Семантическая диагностика IDE](../research/eska-format/analyzer-semantic-diagnostics.md)
добавляет явный `--profile semantic`, общий loader pending saved YAML/BSL,
inter-file dependencies и selected-profile coverage текущего документа.
Ready/editor inputs, отменённые saved reads и прежний syntax default сохраняются.
255 tool tests и все 19 tool/15 root gates прошли; root — 895/14.
На полной копии C1 12 saved pulls дали те же 26 SelfAssign, что checker;
повторы, current BSL/YAML и close проверены в обоих языках, исходники сохранены.
EF-18 остаётся `IN_PROGRESS`: модели, общий memory budget, edits и Explorer ещё в работе.
[Подписки на события](../research/eska-format/analyzer-event-subscriptions.md)
добавлены в canonical inputs. Все 1 084 модели C1/C2/C3 совпали с native reader;
текущий YAML, invalid/close и completion проверены RU/EN. 257 tool tests,
kernel 1105/4, все обязательные gates прошли. Полный checker C1 сохранил 26 findings
и убрал ровно 91 отказ по подпискам; остальных 1 601 issues достаточно для exit 2.
EF-18 остаётся `IN_PROGRESS`, следующие этапы не начаты.
[Регламентные задания](../research/eska-format/analyzer-scheduled-jobs.md)
получили тот же canonical lifecycle. Все 456 моделей C1/C2/C3 совпали с native
reader; current flags/callback и completion проверены RU/EN. 259 tool tests,
kernel 1106/4 и все обязательные gates прошли. Checker C1 сохранил 26 findings
и убрал ещё 83 отказа по моделям; остальные 1 518 issues сохранили exit 2.
EF-18 остаётся `IN_PROGRESS`; расписание и запуск заданий не реализованы анализатором.
[Главные модели подсистем](../research/eska-format/analyzer-subsystems.md)
передают Content и прямые дочерние имена. 197 моделей C1/C2/C3 совпали с native
reader; current YAML и completion проверены RU/EN. 261 tool tests, kernel 1107/4
и все обязательные gates прошли. Checker C1 сохранил 26 findings; вместо 13
отказов по виду осталось семь отказов по nested coverage, всего 1 512 issues.
1 775 вложенных descriptors пока без canonical model; EF-18 — `IN_PROGRESS`.
[HTTP-сервисы](../research/eska-format/analyzer-http-services.md) получили
current маршруты/Handler и согласованный name/file lookup. Все 27 моделей
C1/C2/C3 совпали с native reader; IDE lifecycle проверен RU/EN. 263 tool tests,
kernel 1108/4 и все обязательные gates прошли. Checker C1 сохранил 26 findings,
убрал четыре отказа по виду; остальные 1 508 issues сохранили exit 2.
EF-18 остаётся `IN_PROGRESS`; endpoint runtime и сигнатура Handler не проверены.
[Web-сервисы](../research/eska-format/analyzer-web-services.md) получили current
операции/ProcedureName и имена параметров с тем же name/file lifecycle.
Все 49 моделей C1/C2/C3 совпали с native reader; IDE lifecycle проверен RU/EN.
265 tool tests, kernel 1109/4 и все обязательные gates прошли. Checker C1 сохранил
26 findings, убрал десять отказов по виду; 1 498 issues сохранили exit 2.
EF-18 остаётся `IN_PROGRESS`; SOAP/WSDL/XDTO и сигнатуры не проверены.
[Сервисы интеграции](../research/eska-format/analyzer-integration-services.md)
публикуют каналы/ReceiveMessageProcessing через canonical name/file resolver.
Все три модели и 12 каналов C1/C2/C3 совпали с native reader. Правка binding
обновляет UnusedLocalMethod; saved batch/resident согласованы RU/EN. 268 tool tests,
kernel 1110/4 и все gates прошли; checker C1 сохранил 26 findings и убрал один отказ
по виду, осталось 1 497 issues. Обнаруженный пробел completion закрыт ниже.
EF-18 остаётся `IN_PROGRESS`; сигнатуры и message runtime не проверены.
[Автодополнение сервисов интеграции](../research/eska-format/analyzer-integration-completion.md)
получает объявленные имена через native IDE и общий listing, включая pending и
invalid каналы. RU/EN, shadowing и строки/комментарии проверены; transport schema
сохранена. 270 tool tests, kernel 2626/20 и все 39 gates прошли. Шесть сценариев
C1/C2/C3 не меняли исходники; checker C1 сохранил 26 findings и 1 497 issues.
Версии и graph 206/214 прежние; provenance содержит 495 исходных hashes.
Полная семантика, memory budget, edits и Explorer ещё в работе; EF-18 — `IN_PROGRESS`.
[Адаптер роли](../research/eska-format/role-adapter.md) сохраняет исходные UUID,
три флага, объектные ссылки и RLS из отдельно переданного `RoleData`. Все 1 641
модели C1/C2/C3 совпали с native reader. При приёмке адаптера сервер и checker
ещё не использовали current rights lifecycle; он подключён ниже. EF-18 — `IN_PROGRESS`.
[Общий reader прав](../research/eska-format/role-rights-reader.md) регистрирует
ресурсы по manifest/role composition и читает текущие bytes с прежними path/size
checks. Четыре новых теста и 40 gates прошли; root — 899/14, SDK — прежние 270.
Current resource lifecycle подключён следующим логическим изменением.
[Роли общей сессии](../research/eska-format/analyzer-roles.md) используют current
YAML и отдельно версионированные declared права; resource-only edits и saved
invalidation обновляют свой canonical slot, ошибки не получают defaults/XML fallback.
Full document sync, metadata read/pull и watcher используют тот же clock и quota.
Protocol 0.1 получил resource capability/summary и dependency failure; source
schemas и версии прежние. Все 1 641 модели совпали с native reader, шесть stdio
сценариев прошли. Checker C1 сохранил 26 findings, убрал 212 отказов Role и оставил
1 285 issues; 7 829 hashes и JSON RU/EN совпали. Все 46 gates прошли: 279 tool,
kernel 2627/20, root 899/14, adapter 64/7. Полные модели, memory budget, edits и
Explorer ещё в работе; EF-18 остаётся `IN_PROGRESS`.
Адаптер [`form`](../research/eska-format/form-adapter.md) соединяет текущие
`Form`/`CommonForm` descriptors с явными данными dialog. Все 978 объявленных
общих форм C1/C2/C3 совпали с native reader, 69 unit/8 file tests прошли;
5 consumer/6 adapter/15 root gates сохранены. Canonical form input и ресурсный
lifecycle ещё в работе; EF-18 остаётся `IN_PROGRESS`, EF-12 не начата.
[Reader общих форм](../research/eska-format/form-dialog-reader.md) регистрирует
declared dialogs и читает bounded current bytes; пример адаптера использует
этот вход. Четыре новых теста и 26 gates прошли, root — 903/14. Все 978 моделей
снова совпали с native reader, исходники сохранены. Current lifecycle и
canonical form input ещё в работе; EF-18 остаётся `IN_PROGRESS`.
[Вход формы в ядро](../research/eska-format/canonical-form-input.md) передаёт
текущий slot в BSL module context; pending и непредоставленные формы canonical
корня не читают disk fallback, обычный Designer loader сохранён. Три новых native
теста, kernel 2630/20 и 5 native/20 tool gates прошли. Checker C1 сохранил
26 findings/1 285 issues, JSON RU/EN и 7 829 hashes; graph 206/214 прежний.
Producer dialog в общей сессии ещё не подключён; EF-18 остаётся `IN_PROGRESS`.
[Текущий dialog формы](../research/eska-format/analyzer-forms.md) затем подключён
к сессии и canonical slot. Права/dialog используют общий контроллер, clock и
source quota; current handlers меняют native BSL-диагностику. 978 форм и
1 641 роль C1/C2/C3 совпали с native reader, шесть stdio-сценариев прошли.
Checker C1 снял 83 CommonForm issues, сохранив 26 findings и остальные 1 202 issues.
290 tool tests и все 21 tool/15 root gates прошли; source/protocol schemas прежние.
Nested/binary формы, общий memory budget и прочие условия EF-18 остаются в работе.
[Reader nested форм](../research/eska-format/nested-form-reader.md) сохраняет
полные identity при одинаковых коротких именах и связывает dialog с descriptor
и enclosing YAML. Все 18 186 XML-моделей C1/C2/C3 совпали с native reader;
binary форма C2 отмечена отдельно. Три root tests и один SDK guard, 906/14 root,
291 tool tests и все 6 adapter/21 tool/15 root gates прошли. Checker и current
common-form lifecycle сохраняются; nested session slots/resources ещё в работе.
[Producer nested форм](../research/eska-format/analyzer-nested-forms.md) затем
добавил отдельные slots и текущие dialogs в общую сессию. Правка одного ресурса
сохраняет main/sibling Arc и memo; tree/checker показывают отдельные отказы форм.
Все 18 186 XML-моделей, 978 общих форм и 1 641 роль совпали с native reader.
298 tool tests, 22 tool/15 root gates и шесть stdio-сценариев прошли. Checker C1
снял 242 owner issues, сохранив 26 findings и остальные 960 issues; все hashes
и JSON RU/EN сохранились. Binary формы и остальные условия EF-18 ещё в работе.

[Reader предопределённых данных](../research/eska-format/predefined-reader.md)
связывает declared preserved ресурс со справочником или планом без чтения payload.
Все 305 ресурсов C1/C2/C3 совпали по байтам с native файлами. Четыре root tests
и SDK guard, 910/14 root, 299 tool tests и 22 tool/15 root gates прошли.
Checker C1 сохранил полный JSON RU/EN и квоту. XML parser и current producer
элементов ещё в работе; EF-18 — `IN_PROGRESS`, следующие этапы не начаты.

[Текущие предопределённые элементы](../research/eska-format/analyzer-predefined.md)
подключены к главному `MetadataObject` через общий ресурсный controller.
YAML/resource revisions сохраняют соседа и nested form memos; invalid XML
очищает объект без прежних items. Native completion использует current список.
Все 305 composed objects и 12 756 items C1/C2/C3 совпали с native reader;
978 общих форм и 1 641 роль сохранили parity. Шесть real stdio RU/EN-сценариев
прошли; checker C1 сохранил 26 findings и 960 issues, добавив 85 982 source bytes
(резерв 204 388 259). SDK — 307 tests, adapter — 70/8, kernel — 2 632/20,
root — 910/14; все 23 tool/6 adapter/5 kernel/15 root gates прошли.
Protocol 0.1 добавляет `predefinedDataSync`/`predefined_data`; source schemas,
версии, зависимости и graph 206/214 прежние. API адаптера теперь принимает
items вторым аргументом. Платформа не запускалась. Остальные условия EF-18
ещё в работе; EF-18 — `IN_PROGRESS`, EF-12 и следующие этапы не начаты.


[Адресный completion](../research/eska-format/analyzer-targeted-completion.md)
подготавливает только YAML текущего модуля, прямой native manager target и нужные
primary resources/текущий dialog. Первый request работает без metadata read;
ready/editor input не перечитывается. SDK — 316 tests, kernel — 2 634/20,
root — 910/14 и все 23 tool/5 kernel/15 root gates прошли. Шесть stdio C1/C2/C3
RU/EN сохранили 2/4/1 pending dialogs, текущие native items и hashes; checker C1
сохранил полный JSON, 26 findings, 960 issues и квоту. Protocol 0.1 добавляет
`targetedCompletionMetadata`; source schemas, версии и dependencies прежние.
Общий memory budget и остальные условия EF-18 ещё в работе; следующие этапы не начаты.

[Текущие методы менеджера](../research/eska-format/analyzer-manager-completion.md)
появляются при первом прямом completion без module read: native index выбирает
объявленный body, общий controller сохраняет editor authority и failure states.
Остальные bodies/dialogs остаются pending. SDK — 322 tests, kernel — 2 636/20,
root — 910/14; все 23 tool/5 kernel/15 root gates прошли. Шесть stdio C1/C2/C3
RU/EN проверили 2/2/1 реальных exports и текущий lifecycle; checker C1 сохранил
полный JSON. Protocol 0.1 добавляет `managerModuleCompletion`; graph 206/214,
версии и source schemas прежние. EF-18 остаётся `IN_PROGRESS`.

[Прямой общий модуль](../research/eska-format/analyzer-common-module-completion.md)
читает сначала current YAML, затем доступный body; native ownership исправляет
exports при локальном затенении namespace. SDK — 328 tests, kernel — 2 640/20,
root — 910/14; все 23 tool/5 kernel/15 root gates прошли. Шесть stdio C1/C2/C3
RU/EN проверили 3/1/1 реальных exports и текущие flags/BSL; checker C1 сохранил
полный JSON. Protocol 0.1 добавляет `commonModuleCompletion`; source schemas,
версии, dependencies и graph 206/214 прежние. EF-18 остаётся `IN_PROGRESS`.

[Первый запрос сигнатуры](../research/eska-format/analyzer-targeted-signatures.md)
подготавливает own context и прямой common/manager target через native callee
и общий controller. Read-only call binding сохраняет original argument offset,
порядок overloads и текущие параметры. SDK — 335 tests, kernel — 2 642/20,
root — 910/14; все 23 tool/5 kernel/15 root gates прошли. Двенадцать stdio C1/C2/C3
RU/EN проверили реальные saved signatures, current/invalid/close и watched recovery.
Checker C1 сохранил полный JSON, 26 findings, 960 issues и source reserve.
Protocol 0.1 добавляет `signatureHelp.inputPreparation=direct-member-targets`;
source schemas, версии, dependencies и graph 206/214 прежние. Partial copies
не подтверждают общий heap/RSS budget; платформа не запускалась. EF-18 остаётся
`IN_PROGRESS`, следующие этапы не начаты.

[Первое наведение на метод](../research/eska-format/analyzer-targeted-hover.md)
подготавливает current own context и прямой common/manager target по native callee
token. Позиционная карточка использует typed receiver и общий с completion
ownership predicate; ready foreign module больше не подменяет локальный receiver.
SDK — 341 tests, kernel — 2 646/20, root — 910/14; все 23 tool/5 kernel/15 root
gates прошли. Двенадцать stdio C1/C2/C3 RU/EN проверили реальные saved cards,
current docs/type, invalid/close, loaded-target shadowing и watched recovery.
Checker C1 сохранил полный JSON, 26 findings, 960 issues и source reserve.
Protocol 0.1 добавляет `hover.inputPreparation=direct-member-callees`; source/result
schemas, версии, dependencies и graph 206/214 прежние. Partial copies не подтверждают
общий heap/RSS budget; платформа не запускалась. EF-18 — `IN_PROGRESS`, следующие
этапы не начаты.

[Границы инструментов](../tool-boundaries.md) сохраняются: общая модель,
отдельные project/build, один IDE/batch host, будущие fmt/VCS/release/debug.
T83/T84 остаются минимальными предпосылками профиля внешних объектов EF-14;
автоматические релизы T56/T32 в MVP не входят. Установщики/workflows этой
правкой плана не изменены.

Направление [отдельных .sdbl](../research/eska-format/sdbl-queries.md), принятое
2026-10-10, теперь входит в MVP: EF-23 — хранение и обратная сборка до анализатора,
EF-27 — анализ контекста/зависимостей после EF-18. Это разделение устраняет
зависимость структуры файлов от готовности языкового сервера.
EF-14/EF-15 переведены из отложенных в обязательные, EF-25 добавляет повторный
импорт; EF-19 больше не является условием CLI, поставки и приёмки MVP.

По запросу от 2026-09-28 в `feat/cli` завершена T82: создание и обновление
проектов из CF/CFE/EPF/ERF. Контракт и состояние — в
[project creation](02-project-creation.md#t82--создание-и-обновление-из-файлов-1с).

Для прежнего Designer XML **локальный CLI MVP и workspace готовы**; это не
приёмка нового MVP ESKA. По решению от 2026-09-17 в ветке `feat/ide` велась
подготовка `eska` и первой версии
VS Code extension для навигации по Designer XML.

- `DONE`: чистый минимальный Rust CLI;
- `DONE`: локализация `ru-RU` / `en-US`, включая `--help`;
- `DONE`: `T01` — минимальная доменная модель проекта;
- `DONE`: `T02` — схема и загрузка `eska.toml`;
- `DONE`: `T03` — project discovery, проверка исходников и ошибки CLI;
- `DONE`: `T05` — встроенные шаблоны для четырёх типов проектов;
- `DONE`: `T04` — `eska new`, клавиатурный TUI, безопасное создание и минимальный Git init;
- `DONE`: `T06` — `eska init`, подключение существующих исходников;
- `DONE`: `T07` — repository layer: HEAD, refs, история, status и changed paths;
- `DONE`: `T08` — workflow policy model, preset overrides и декларативный план;
- `DONE`: `T09` — Trunk preset;
- `DONE`: `T10` — Git Flow preset;
- `DONE`: `T11` — GitHub Flow preset;
- `DONE`: `T12` — `eska status`;
- `DONE`: `T13` — `eska start`;
- `DONE`: `T14` — object-aware human-представление `eska diff`;
- `DONE`: `T15` — `eska save`;
- `DONE`: `T16` — `eska clone` через `gix`;
- `DONE`: `T17` — реализованные VCS-операции переведены на gix-first слой;
- `DONE`: `T18` — локальная история commit/task без fetch и изменения repository;
- `DONE`: `T19` — Designer XML logical object model;
- `DONE`: `T20` — reusable semantic `ChangeSet`;
- `DONE`: `T21` — semantic diff;
- `DONE`: `T22` — генератор commit message;
- `DONE`: `T34` — безопасное переключение между существующими задачами;
- `DONE`: `T40` — локальное завершение задачи;
- `DONE`: `T28` — сборка `.cf`, `.cfe`, `.epf`, `.erf` через
  настраиваемый `ibcmd`, глобальный machine config и одноразовый выбор
  установленной платформы;
- `DONE`: `T42` — спецификация и feasibility-прототип patch-extension `.cfe`;
- `DONE`: `T43` — production-команда `eska patch` для консервативного набора
  изменений методов общих модулей;
- `DONE`: `T25` — чтение и точечное изменение версии проекта 1С;
- `DONE`: `T44` — workspace config, model и discovery;
- `DONE`: `T45` — версионирование workspace members;
- `DONE`: `T46` — сборка workspace members;
- `DONE`: `T47` — VCS UX workspace members;
- `DONE`: `T48` — onboarding workspace members;
- `DONE`: `T29` — `eska doctor`;
- `DONE`: `T49` — стабильные JSON-ошибки и обратимые пути;
- `DONE`: `T50` — анализ только затронутых объектов и явный fallback;
- `DONE`: `T51` — предварительный просмотр `save`;
- `DONE`: `T52` — предварительный просмотр `build`;
- `DONE`: `T54` — паспорт собранного артефакта;
- `DONE`: `T58` — настройка основной production-ветки Git Flow без изменения
  базы feature-веток;
- `DONE`: `T59` — читаемый semantic-вывод `eska diff` с координатами методов,
  устранением дублей и порядком метаданных Конфигуратора;
- `DONE`: `T35` — автоматическое сохранение и восстановление незакоммиченного
  состояния при переключении задач;
- `DONE`: `T83` — три и четыре компонента версии проекта;
- `DONE`: `T84` — версия внешнего объекта через модуль;
- `PLANNED`: `T56` — автоматический план версии релиза 1С, подготовка PR/MR — T32;
  задача сохранена в backlog после переключения приоритета на IDE;
- `IN-PROGRESS`: `T57` — обновление установленного бинарника командой `eska update`
  по отдельному запросу пользователя, без ожидания T56;
- `PLANNED`: `T55` — переносимый стенд реализован, завершение ожидает запусков
  на Windows/macOS и приёмки с настоящей платформой 1С;
- `NEEDS-SPEC`: `T53` — завершение задачи после squash/rebase;
- `PLANNED`: `T37` — `sync` / `continue` / `abort` после T57;
- `PLANNED`: `T26` — `eska fmt`, первый приоритет после нового MVP EF-16;
- `DONE`: `T60` — модель метаданных и логическая identity;
- `DONE`: `T61` — Designer resolver и открытие через обязательный manifest;
- `DONE`: `T62` — fixtures, матрица типов и большой стенд;
- `DONE`: `T63` — выбор XML parsing по измерениям;
- `DONE`: `T64` — разбор Designer XML в модель метаданных;
- `DONE`: `T65` — схемы дерева Конфигуратора;
- `DONE`: `T66` — read-only metadata workspace API;
- `DONE`: `T67` — lazy cache и инкрементальное обновление;
- `DONE`: `T75` — индекс и API поиска;
- `DONE`: `T68` — дисковый кеш и замеры производительности;
- `DONE`: `T69` — спецификация постоянного IDE protocol;
- `DONE`: `T70` — постоянный IDE процесс;
- `DONE`: `T76` — основа расширения и подключение к backend;
- `DONE`: `T77` — дерево, модули и открытие исходников;
- `DONE`: `T78` — поиск и переход к результату в расширении;
- `DONE`: `T79` — фильтр пустых разделов корня и внутри «Общие», исходная настройка;
- `DONE`: `T80` — иконки метаданных;
- `IN-PROGRESS`: `T81` — итоговая приёмка и VSIX;
- `PLANNED`: `T41`, декомпозированная в `T76–T81`, — расширение VS Code с деревом,
  поиском, фильтром, иконками и устанавливаемым VSIX;
- `DONE`: `T71` — архитектура точечного редактирования;
- `IN-PROGRESS`: `T72–T73` — первый набор свойств доступен через общий CLI JSON
  и IDE API 1.8; остаются дополнительные mappings и межплатформенная приёмка;
  `T74` — переименование `IN-PROGRESS`, add/remove `DEFERRED`;
- `T23` test backend и `T39` locking отложены до проверки этого MVP в реальной
  работе.

Практический MVP замыкает основной пользовательский цикл без обязательных
test backend, locking и публикации через `eska`:

```text
start -> status/diff -> save -> switch/return -> finish
                                      |
                                    build .cf
```

| Пользовательская потребность | Текущее покрытие |
|---|---|
| Создать и начать задачу | `eska start <task>` — `DONE` |
| Увидеть изменённые файлы и объекты | `eska status`, `eska diff` — `DONE` |
| Создать commit | `eska save` — `DONE` |
| Переключиться и позднее вернуться | `eska switch` — `DONE` |
| Завершить задачу | `eska finish` — `DONE` |
| Собрать полный нативный артефакт | `eska build` → `.cf/.cfe/.epf/.erf` — `DONE` |
| Собрать patch-extension из delta | `eska patch` → `.cfe` — `DONE` для ограниченного набора методов |

T42 подтвердил узкий сценарий замены метода общего модуля на 8.3.27.2325. T43
зафиксировал консервативный allowlist и добавил отдельную команду с обязательной
проверкой BSL и применимости пакетным Конфигуратором. Подробности и ограничения —
[в результате T42/T43](t42-patch-extension.md).

T35 добавила автоматические полки при `switch` и явные команды
`shelve` / `unshelve` / `shelves`. T58 завершила настройку production-ветки Git Flow с
сохранением `develop` базой обычных feature-веток. T59 сделала semantic-вывод
однозначным, подавила противоречивые события и добавила координаты методов.
Поиск T78, фильтр T79 и иконки T80 реализованы. В работе T81: итоговая приёмка и VSIX. T56 остаётся в backlog.

Структурный рефакторинг после T06: команды сгруппированы в `src/cli/commands/`,
операции проекта — в `src/project/`, TOML-схема отделена от проверенных настроек,
TUI разделён на обработку клавиш, отрисовку и управление терминалом. CLI и config
сохранены; T07 не реализуется в рамках рефакторинга. Актуальные пути и границы:
[карта исходного кода](../architecture.md).

Перед началом любой задачи нужно сверять статус с фактическим кодом и менять его
на `DONE` только после выполнения критериев готовности.

Ревью и рефакторинг по запросу 2026-09-24 — **DONE**: исправлено структурное
сравнение XML, ускорено пакетное удаление поисковых ветвей, `diff` и `semantic`
разделены по обязанностям с сохранением CLI/JSON и публичных Rust-путей.
Результаты, охват и ограничения — [в отчёте ревью](../reviews/2026-09-24.md).
Очередность продуктовых задач этим рефакторингом не изменяется.

## Статусы

- `DONE` — реализовано и проверено в текущем репозитории;
- `NEXT` — следующая задача, которую можно брать без дополнительных решений;
- `PLANNED` — задача определена, но ждёт зависимостей;
- `DEFERRED` — намеренно отложенная возможность;
- `NEEDS-SPEC` — идея зафиксирована, но перед реализацией нужна отдельная спецификация.

## Ближайший порядок выполнения

Порядок изменён пользователем 2026-10-11. Каждая задача принимается отдельно;
эта правка документации не запускает реализацию очереди.

| Порядок | Задачи | Результат |
|---|---|---|
| 1 | EF-24 (`DONE`) → EF-12 (`DONE`) → EF-23 (`DONE`) → T83/T84 (`DONE`) → EF-14 (`DONE`) → EF-15 (`DONE`) → EF-25 (`IN_PROGRESS`) → EF-20 | Завершённая структура, миграция, new/init, повторный импорт и сборка CF/CFE/EPF/ERF |
| 2 | Продолжить EF-18 → EF-27 | Общий IDE/batch анализатор и контекстная проверка .sdbl |
| 3 | EF-11 → EF-21 → EF-16 | Установленный Explorer: дерево, поддержка, подсветка, анализ; приёмка MVP |
| 4 | T26 → форматирование в EF-22 | fmt для CLI и IDE |
| 5 | EF-19 → EF-13 | VCS и проверка слияния форм |

RLS/длинные выражения (EF-26), автоматические релизы и другие направления
остаются после этих приоритетов. Полный typed writer СКД, визуальный редактор
форм и EDT не являются условиями MVP. Подробные зависимости и границы —
в [очереди EF](15-eska-format-experiments.md).

### Предыдущая поставка Designer XML

Последовательность от 2026-09-17 сохранена для истории. Она не заменяет очередь
нового формата; завершённые задачи остаются `DONE`, T81 сохраняет незакрытую
приёмку прежней IDE-поставки. EF-11 переиспользует готовые возможности Explorer.

| Порядок | ID | Результат |
|---|---|---|
| 1 | T60 | Модель метаданных и стабильная identity — `DONE` |
| 2 | T61 | Manifest, Designer resolver и переход к XML/BSL — `DONE` |
| 3 | T62 | Fixtures, матрица типов и большой стенд — `DONE` |
| 4 | T63 | Выбор XML parser по замерам и проект бюджетов — `DONE` |
| 5 | T64 | Разбор Designer XML по запросу — `DONE` |
| 6 | T65 | Схемы дерева, первая группа модулей и пустые коллекции — `DONE` |
| 7 | T66 | Read-only metadata API — `DONE` |
| 8 | T67 | Lazy cache и обновление затронутых объектов — `DONE` |
| 9 | T75 | Поисковый индекс и core search API — `DONE` |
| 10 | T68 | Дисковый кеш и измеренная производительность backend — `DONE` |
| 11 | T69 | Контракт постоянного IDE protocol — `DONE` |
| 12 | T70 | Процесс `eska ide --stdio` — `DONE` |
| 13 | T76 | Основа расширения и подключение к `eska` — `DONE` |
| 14 | T77 | Дерево, модули и открытие исходников — `DONE` |
| 15 | T78 | Поиск и переход к результату — `DONE` |
| 16 | T79 | Фильтр пустых групп и исходная настройка — `DONE` |
| 17 | T80 | Иконки метаданных — `DONE` |
| 18 | T81 | Приёмка скорости/UI и VSIX; закрытие T41 — `IN-PROGRESS` |

Для всех четырёх типов проекта обязателен `eska.toml`: его проверенный тип
определяет схему дерева. `.bin` без BSL скрываются. Большой стенд —
`<repo>/../eska-playground/big_configuration_temp`; **сборку этого проекта
не запускать**, использовать только чтение метаданных и замеры. Изменения XML
для тестов выполняются на небольших собственных fixtures. Подробные границы и
неподтверждённые UX-предложения — в [плане расширения](14-vscode-extension.md).

T57 выполняется по отдельному запросу пользователя. Прежняя последовательность
T56 → T37 → T55 → T26 больше не задаёт приоритет: T26 идёт после нового MVP,
остальные задачи остаются в backlog. Доступный runner T55 не является поводом
собирать большой IDE-стенд.
T71 завершена; T72–T73 реализованы в ограниченной области и продолжаются.
В T74 пользователь включил переименование со ссылками; add/remove остаются отложенными.

T53 нельзя начинать с ослабления проверки интеграции: сначала снимается
`NEEDS-SPEC`. Уточнение её контракта не блокирует независимые T54–T55.
`publish`, environments, `apply/run` и остальные плановые этапы остаются в backlog;
T23 test backend и T39 locking сохраняют `DEFERRED`.

Детальные контракты улучшений: [T49–T55](12-current-functionality.md).
Контракт автоматической подготовки версии релиза: [T56](10-delivery-and-integrations.md).
Контракт обновления установленного CLI: [T57](10-delivery-and-integrations.md).
Этот план разрешает только отдельно запрошенную задачу, не автоматический запуск
всей очереди.

## Последовательность metadata / IDE

Детальные контракты: [метаданные Designer XML и IDE protocol](13-metadata-and-ide.md).
Это развитие T19/T50 с переиспользованием существующих `ObjectId`, discovery и
mapping, а не повторная реализация завершённых задач. Задачи клиента описаны
в [T76–T81](14-vscode-extension.md), они относятся к прежней поставке Designer XML.
Интеграция нового формата и языка принимается отдельно в EF-11.

| Milestone | Задачи | Результат |
|---|---|---|
| Metadata V0.1 — Core model | T60–T62 | Модель, logical identity, resolver, fixtures |
| Metadata V0.2 — Designer XML parser | T63–T64 | Стратегия parsing и parser четырёх типов проектов |
| Metadata V0.3 — Configurator tree | T65–T66 | Schema, golden tests и read-only API |
| Metadata V0.4 — Incremental workspace | T67, T75, T68 | Lazy cache, поиск, обновления и benchmarks |
| IDE V0.1 — Read-only protocol | T69–T70 | Спецификация и постоянный `eska ide --stdio` |
| VS Code V0.1 — Metadata explorer | T41: T76–T81 | Дерево, модули, поиск, фильтр, иконки и VSIX |
| Metadata V0.5 — Existing object editing | T71–T73 | Safe XML patching и изменение существующих элементов |
| Metadata V0.6 — Structural editing | T74 | Переименование и согласованная запись; add/remove отложены |

Metadata/IDE API первой поставки полностью read-only. T41 — сводная задача
клиента, закрываемая результатом T76–T81 после T70. EDT, MCP, AI, собственный
BSL LSP и визуальный редактор форм в это направление не входят.

## Реестр задач

Таблица сохраняет порядок ID; фактическая очередь приведена выше.

| ID | Статус | Задача | Подробности |
|---|---|---|---|
| B00 | DONE | Clean baseline | [00-foundation.md](00-foundation.md) |
| B01 | DONE | Локализация CLI | [00-foundation.md](00-foundation.md) |
| T01 | DONE | Доменная модель `Project` | [01-project-foundation.md](01-project-foundation.md) |
| T02 | DONE | Схема и загрузка `eska.toml` | [01-project-foundation.md](01-project-foundation.md) |
| T03 | DONE | Project discovery и validation | [01-project-foundation.md](01-project-foundation.md) |
| T04 | DONE | `eska new` | [02-project-creation.md](02-project-creation.md) |
| T05 | DONE | Built-in templates | [02-project-creation.md](02-project-creation.md) |
| T06 | DONE | `eska init` | [02-project-creation.md](02-project-creation.md) |
| T07 | DONE | Repository layer (`gix`; документированный Git capability fallback) | [03-repository-workflow.md](03-repository-workflow.md) |
| T08 | DONE | Workflow policy model | [03-repository-workflow.md](03-repository-workflow.md) |
| T09 | DONE | Trunk preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T10 | DONE | Git Flow preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T11 | DONE | GitHub Flow preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T12 | DONE | `eska status` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T13 | DONE | `eska start` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T14 | DONE | `eska diff` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T15 | DONE | `eska save` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T16 | DONE | `eska clone` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T17 | DONE | Gix-first миграция реализованных VCS-операций | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T18 | DONE | `eska history` | [05-safe-vcs.md](05-safe-vcs.md) |
| T19 | DONE | Designer XML object model | [06-locking-and-xml.md](06-locking-and-xml.md) |
| T20 | DONE | Semantic `ChangeSet` | [07-semantic-changes.md](07-semantic-changes.md) |
| T21 | DONE | Semantic diff | [07-semantic-changes.md](07-semantic-changes.md) |
| T22 | DONE | Генератор commit message | [07-semantic-changes.md](07-semantic-changes.md) |
| T23 | DEFERRED | Спецификация test backend | [08-quality.md](08-quality.md) |
| T24 | PLANNED | `affected` analysis | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T25 | DONE | Versioning проекта 1С | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T26 | PLANNED | `eska fmt` (после MVP EF-16, перед новым VCS) | [08-quality.md](08-quality.md) |
| T27 | PLANNED | `eska check` | [08-quality.md](08-quality.md) |
| T28 | DONE | Build через `ibcmd` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T29 | DONE | `eska doctor` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T30 | PLANNED | Development environments | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T31 | PLANNED | `apply` / `run` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T32 | PLANNED | `eska-release`: подготовка PR/MR и публикация | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T33 | PLANNED | CI integration | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T34 | DONE | `eska switch` | [05-safe-vcs.md](05-safe-vcs.md) |
| T35 | DONE | `shelve` / `unshelve` / `shelves` и автоматическое восстановление при `switch` | [05-safe-vcs.md](05-safe-vcs.md) |
| T36 | PLANNED | `eska restore` | [05-safe-vcs.md](05-safe-vcs.md) |
| T37 | PLANNED | `eska sync` / `continue` / `abort` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T38 | PLANNED | `eska publish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T39 | DEFERRED | Locking объектов | [06-locking-and-xml.md](06-locking-and-xml.md) |
| T40 | DONE | `eska finish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T41 | PLANNED | eska: 1C Explorer — сводная задача T76–T81 | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T42 | DONE | Спецификация и прототип patch-extension `.cfe` из разницы веток | [t42-patch-extension.md](t42-patch-extension.md) |
| T43 | DONE | Генерация patch-extension для ограниченного набора методов | [t42-patch-extension.md](t42-patch-extension.md) |
| T44 | DONE | Workspace config, model и discovery | [11-workspaces.md](11-workspaces.md) |
| T45 | DONE | Versioning workspace members | [11-workspaces.md](11-workspaces.md) |
| T46 | DONE | Build workspace members | [11-workspaces.md](11-workspaces.md) |
| T47 | DONE | Workspace-aware `status`, `diff` и `save` | [11-workspaces.md](11-workspaces.md) |
| T48 | DONE | `new` и `init` для workspace members | [11-workspaces.md](11-workspaces.md) |
| T49 | DONE | Стабильные JSON-ошибки и обратимые пути | [12-current-functionality.md](12-current-functionality.md) |
| T50 | DONE | Анализ затронутых объектов и явный fallback | [12-current-functionality.md](12-current-functionality.md) |
| T51 | DONE | Предварительный просмотр `save` | [12-current-functionality.md](12-current-functionality.md) |
| T52 | DONE | Предварительный просмотр `build` | [12-current-functionality.md](12-current-functionality.md) |
| T53 | NEEDS-SPEC | Завершение задачи после squash/rebase | [12-current-functionality.md](12-current-functionality.md) |
| T54 | DONE | Паспорт собранного артефакта | [12-current-functionality.md](12-current-functionality.md) |
| T55 | PLANNED | Переносимые проверки сборки; ожидает внешней приёмки | [12-current-functionality.md](12-current-functionality.md) |
| T56 | PLANNED | Автоматическая подготовка версии релиза 1С | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T57 | IN-PROGRESS | Обновление установленного `eska` | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T58 | DONE | Настраиваемая основная production-ветка Git Flow | [03-repository-workflow.md](03-repository-workflow.md) |
| T59 | DONE | Читаемый semantic-вывод `eska diff` с координатами методов | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T60 | DONE | Модель метаданных и логическая identity | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T61 | DONE | Designer path resolver | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T62 | DONE | Набор Designer XML fixtures | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T63 | DONE | Выбор стратегии XML parsing | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T64 | DONE | Разбор Designer XML в модель метаданных | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T65 | DONE | ConfiguratorSchema и ConfiguratorTree | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T66 | DONE | Read-only metadata workspace API | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T67 | DONE | Lazy index, cache и инкрементальное обновление | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T68 | DONE | Производительность metadata workspace | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T69 | DONE | Спецификация постоянного IDE protocol | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T70 | DONE | Постоянный read-only IDE процесс | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T71 | DONE | Архитектура safe XML patching | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T72 | IN-PROGRESS | Точечный XML patch и безопасная запись | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T73 | IN-PROGRESS | Редактирование существующих элементов | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T74 | IN-PROGRESS | Переименование со ссылками; add/remove отложены | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T75 | DONE | Поиск по метаданным | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T76 | DONE | Основа расширения и подключение к eska | [14-vscode-extension.md](14-vscode-extension.md) |
| T77 | DONE | Дерево проектов, модули и открытие исходников | [14-vscode-extension.md](14-vscode-extension.md) |
| T78 | DONE | Поиск в расширении и переход к результату | [14-vscode-extension.md](14-vscode-extension.md) |
| T79 | DONE | Фильтр пустых групп и настройка исходного режима | [14-vscode-extension.md](14-vscode-extension.md) |
| T80 | DONE | Иконки метаданных | [14-vscode-extension.md](14-vscode-extension.md) |
| T81 | IN-PROGRESS | Производительность интерфейса, приёмка и VSIX | [14-vscode-extension.md](14-vscode-extension.md) |
| T82 | DONE | Создание и обновление из файлов 1С | [02-project-creation.md](02-project-creation.md) |
| T83 | DONE | Версии проекта `x.y.z` и `x.y.z.build` | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T84 | DONE | Обязательная `Версия()` внешнего отчёта/обработки | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |

Отложенные и пока недостаточно определённые возможности перечислены в
[99-deferred.md](99-deferred.md). Общие правила для каждой задачи находятся в
[working-agreement.md](working-agreement.md).

## Как обновлять трекер

1. В начале работы пометить одну задачу `NEXT` как `IN-PROGRESS` в её файле и таблице.
2. Не расширять scope за границы задачи; новые идеи записывать в `99-deferred.md`.
3. После реализации выполнить общий Definition of Done.
4. Пометить задачу `DONE`, записать принятые решения и сделать следующую
   разблокированную задачу `NEXT`.
