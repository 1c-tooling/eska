# Набор приёмки формата ESKA

[Структура и MVP](project-layout.md) фиксируют приоритет от 2026-10-11:
структура и файловый цикл CF/CFE/EPF/ERF → анализатор → Explorer → fmt → VCS.

[Дополнительные профили EF-14 приняты](external-projects.md): независимые
CFE/EPF/ERF циклы и выполнение непустых ERF на 1С 8.3.27.2325.
[Обновление принятой базы EF-15](base-update.md) принято для standalone/workspace
с независимыми pin и контролируемыми native поставками.
EF-25 принята по [итоговой сверке повторного импорта](borrowed-native-reimport.md);
EF-20 принята по [CLI-приёмке](cli-workflow-acceptance.md); текущая EF-18
продолжает общий анализатор и checker. Далее сохранена история срезов.
EF-25 добавляет [распаковку артефактов](native-unpack.md),
[историю исходников и трёхсторонний план](import-ancestry.md),
[применение и recovery выбранного проекта/member](source-import-recovery.md).
[Native staging → project plan](native-project-import.md) проверены на четырёх
настоящих artifact/import/build/dump циклах под прежним CI pin.
Новая карта и самостоятельный native CLI подключены в EF-25;
согласованное обновление IDE в том срезе не принималось. Общий CLI и анализ остаются
отдельными задачами.
[Editor bindings после импорта](post-import-edit-bindings.md) проверяют текущие
пути/UUID под прежним pin и блокируют stale saved edits. JSON preview показывает
support изменённых UUID; четыре native edit/build/dump прошли. Refresh работающего
IDE и unsaved вкладки при импорте в том срезе не принимались.
[Guard анализатора](analyzer-import-guard.md) проверяется перед dispatch и wire
publication, освобождает старый host с последним отправленным snapshot и
сохраняет caller-owned dirty buffers для явного replay. Настоящий CF import
прошёл с открытыми документами; автоматический клиентский restart в том срезе не принимался.
[Установленный workspace import/recovery](installed-native-workspace.md) позже
проверил all4 addition/deletion/rename и обе recovery actions: 20 случаев,
63 Save и 24 orphan drafts. Dirty сосед с теми же UUID/bytes получает собственный
ACK до переключения и не передаётся выбранному member; остальные members и
workspace controls сохранены. Позднее [borrowed reimport](borrowed-native-reimport.md)
закрыл EF-25; общий CLI workflow остаётся EF-20.
EF-23 — [DONE в принятом профиле](query-acceptance.md).
[Создание полного Queries проекта](query-project-creation.md) подключает
managed forms/plain queries к `eska-project import` для всех четырёх типов.
Old layouts/defaults сохранены; общий new/init и оболочка остаются EF-20.
[Initial ancestor](initial-import-ancestor.md) Queries creation хранится отдельно
от поддержки; независимый digest нужен первому journal, а Git transport сохраняет
raw initial/history/state bytes для standalone/member.
[Делегирование `eska build`](shell-build-delegation.md) подключает одиночную
сборку ESKA к совместимому companion. Native all4 build/dump и первый import
с initial ancestor прошли, сохранив local BSL/pin/settings и прежний артефакт.
[Минимальный `eska-project new`](minimal-project-creation.md) создаёт четыре
типа с fresh native UUID и обеими схемами версии через общий Queries pipeline.
Общий shell new/init и приёмка MVP остаются в работе.
[Оболочка new](shell-project-creation.md) связывает пустое и native создание с
workflow/Git/workspace и expected-byte receipt; [init](project-initialization.md)
подключает existing Queries source. [Queries migration](query-project-migration.md)
сохраняет raw history при no-op и явно отказывает несовместимому layout change.
[LocalSyntax четырёх типов](analyzer-local-syntax.md) проверяет BSL и symbols через
общую сессию, сохраняя явную неполноту semantic context.
[Declared `.sdbl` syntax](analyzer-query-syntax.md) подключён к той же сессии,
watcher, quota и native parser без BSL wrapper. Контекстная EF-27 ещё нужна.
[Диагностика по текущим metadata](analyzer-query-metadata.md) добавляет отдельный
queryAnalysis opt-in и тот же API для semantic checker. Per-file CF/CFE visibility
и current failures сохранены; параметры, output fields и consumers ещё нужны.
[500 циклов current inputs](current-input-stress.md) малых EPF/ERF проверили
hashes, общий reserve и replay после отказа квоты на RU/EN.
[Большой standalone CF](large-cf-current-input.md) проверен release-сборкой:
3617 owners, холодное/тёплое открытие и 500 current field updates на RU/EN,
отдельный RSS и quota/replay. Большой настроенный CFE/base и общий RSS budget
остаются отдельной приёмкой.
[Unicode metadata ranges](analyzer-unicode-ranges.md) исправляют падение полного
checker на кириллице/BOM. Все 1992 BSL modules большого CF обработаны на RU/EN,
выбранный модуль совпал с IDE; exit 2 сохраняет явную неполноту моделей.
[Общая очистка native-кешей](analyzer-query-cache-lifecycle.md) охватывает
успех/ошибку всех language methods под прежним deadline, сохраняя текущие
inputs и editor parse hints. Общий heap/RSS budget остаётся отдельной задачей.
[Kernel owners](canonical-module-owners.md) берут object/register/common/service
metadata из текущих slots без XML fallback.
[CF/CFE composition](analyzer-cf-cfe-composition.md) связывает явные source roots
и dependency closure в одном SDK host. Current UUID/overlays, foreign workspace
controls, холодная navigation и native references проверены на синтетических
Queries-проектах настоящим framed transport: SDK 184/464 и native 90 tests.
Это отдельный срез EF-18 без claim полной семантики `.sdbl`, платформы или MVP.
[Own EPF/ERF context](external-own-context.md) использует реальные external модели,
сохраняя явную неполноту owning configuration; YAML diagnostics доступны отдельно.
[Navigation/policy](analyzer-navigation.md) передают declared tree и права чтения
через тот же host; independent pin не зависит от semantic scope.
[Первая установленная приёмка Explorer](explorer-analyzer-foundation.md) проверяет
один analyzer host, readonly readers и сохранение черновиков для четырёх типов.
[Текущая навигация](explorer-current-navigation.md) добавляет поиск/reveal,
current labels и отзыв policy; editable opening и остальная матрица EF-11/EF-16
продолжаются.
[Синтаксис `.sdbl` в установленном Explorer](explorer-query-syntax.md) прошёл
all4 на том же host: sync/diagnostics, dirty reveal, watch/restart/import guard.
Полный Node gate — 257/257 без skipped; metadata/consumer semantics остаётся EF-27.
[Диагностика по текущим metadata в Explorer](explorer-query-analysis.md) прошла
installed CF/configured-CFE и native CF; standalone native CFE без базы явно
отказала semantic initialize и сохранила отдельный syntax. Node — 310/310;
последующая проверка CR-координат — 311/311. Consumer findings ещё не подключены.
[Комментарий в текущем буфере](explorer-comment-buffer.md) прошёл installed
CF/configured-CFE: preview, native edit/undo/redo и restart/import replay.
284 Node tests без skipped; обычный Save и полное editable opening ещё нужны.
[Листовая structural coverage](structural-metadata-coverage.md), включая
CommonPicture/CommandGroup, проверяет текущие
descriptor fields без фиктивной canonical модели или проверки СКД.
[Параметры сеанса](analyzer-session-parameters.md) добавляют настоящие typed
SessionParameters fields через per-name current slots, native root ownership
и execution contexts; одноимённые query parameters не типизируются по ним.
[Current form/DCS view](query-context-read-view.md#один-текущий-источник-в-sdk)
использует принятый owner snapshot и отдельный current XML input без saved
fallback; полный SDK — 197/488 tests, consumer semantics ещё нужны.
[Native output и прямые bindings](query-output-consumers.md) связывают текущий
query model с DCS/DynamicList в library API и одном отчёте IDE/checker.
Независимый queryConsumers opt-in сохраняет прежний pull; current XML и
authoring ranges проверены на RU/EN. Параметры пока occurrences, полная EF-27
ещё нужна.
[Установленный consumer client](explorer-query-consumers.md) прошёл 323 Node tests,
CF/configured-CFE/native-CF XML Problems/repair/replay без saved fallback.
Полная семантика и Save остаются отдельными границами.
[Текущие входы базы и зависимостей](analyzer-foreign-current-inputs.md) устраняют
saved metadata при открытом foreign draft выбранной CFE: explicit declarations,
root-qualified ACK и прежний replay, 334 Node tests и real-process RU/EN guards.
[Main source import](shell-source-import.md) делегирует preview/write/recovery
одному совместимому tool с независимыми pins и сохранением streams/exit.
[Оболочка check](shell-check-delegation.md) передаёт явный профиль совместимому
checker; восемь all4 RU/EN случаев совпали с direct streams/exit без изменения входов.
[Локальный комплект пяти CLI](local-tool-bundle.md) прошёл настоящую release
сборку, empty-PATH smoke и выборочное обновление с сохранением прежнего набора.
Размеры и normal dependency graphs измерены; обычный install/update/remove ещё нужен.
[Необязательный Explorer в комплекте](local-tool-bundle-explorer-results.json)
прошёл полную и analyzer-only упаковку без Cargo, empty-PATH verify RU/EN и
проверку точных bytes установленной пары. Все 26 Python groups прошли;
manifest/hash не подменяет runtime-приёмку, глобальная установка ещё нужна.
T83 — [DONE по CF/CFE циклу двух схем версии](project-version.md);
T84 — [DONE по версиям EPF/ERF](external-version.md): экспортная `Версия()`,
CLI/миграция и платформенная приёмка двух схем; [синтаксический контракт](external-version-syntax.md).
[Native slots/query codec](native-query-codec.md)
проверены на 150 запросах C1; [ссылки версии 1](query-references.md) перенесли
те же values в plain `.sdbl` с disk no-op/edit/reparse.
[Проектная карта 5](query-project.md) подключает opt-in import/export и адресный
reader. [Protected writer и source map 4](query-protected.md) сохраняют supplier
права и дают точные plain координаты. [Точечный editor](query-edit.md) меняет
один `.sdbl` с проверкой Resource прав/pin/conflicts. [Явная миграция](query-migration.md)
раздельно переносит source/base и сохраняет settings проекта/workspace.
Изменённые условия СКД/списка прошли CF/build/dump и реальное исполнение;
контекстный analysis scope остаётся EF-27, CLI defaults сохранены.
[Current query resource view](query-context-read-view.md) проверяет supplied
owner/resource inputs layouts 5/6 без файлового I/O и `.sdbl` reads; precision
XML/YAML сообщается явно. Языковой resolver и consumer checks ещё нужны.
[EF-12 принята](managed-form-acceptance.md)
для configuration/C1/logform 2.20 на 1С 8.3.27.2325.
[Native ID/ссылки](managed-form-inventory.md) и
[общая модель layout](managed-form-layout.md) и
[обратимый body codec](managed-form-body.md) проверены на 823 формах C1;
[readable properties body v2](managed-form-properties.md) — на 12 317 values,
чтение v1 сохранено. [Единый form.yaml](managed-form-project.md) объединяет
metadata descriptor и dialog body через карту версии 4; project reader/export
проверены на всех 823 формах C1. [Protected writer и source map](managed-form-protected.md)
выбирают body по принятому metadata UUID: 183 формы получили разрешённые правки,
637 запрещённых форм восстановлены из базы. [Точечный editor](managed-form-edit.md)
сохраняет property/layout edits через CST и общий atomic writer.
[Явная миграция](managed-form-migration.md) переносит source, pinned base,
standalone проект и workspace в map 4. Независимый native control и ESKA
прошли CF/build/dump: изменённые Title/layout сохранены, остальные native bytes
равны с единственным исключением проверенных configVersion.
[EF-24 принята](structure-acceptance.md)
для configuration/Designer 2.20. [28 новых typed-скаляров](typed-scalars.md)
подготовлены в общем core. [Самостоятельные descriptors](descriptor-layout.md) добавляют
обратную карту версии 2, адресное чтение/редактирование и явный план миграции
исходников. [Отдельный план trusted base](accepted-layout.md) сохраняет pinned
values и политику, возвращая новый pin. [Карта версии 3](resource-layout.md)
отделяет физические payload paths от native identity, сохраняя байты, защиту,
reader/source map и контекст SDK. [Явный CLI import](readable-project-import.md)
выбирает читаемую структуру через `--layout readable`; default family сохранён.
[Миграция standalone-проекта](project-migration.md) связывает рабочие исходники
и pinned base в новую readable копию с preview, repeat/recovery и отдельным pin.
[Миграция workspace](workspace-migration.md) сохраняет exact root/member manifests
и наследование, требует отдельные approved pins и блокирует частичную публикацию.
[Порядок существующих детей](child-order.md) редактируется по native UUID и
декларациям с проверкой поддержки/freeze владельца и перемещённых элементов;
состав и позиции native классов сохраняются. [Новые EnumValue](enum-value-creation.md)
создаются через права принятого владельца по закрытому native профилю с проверкой
UUID/имён и Working координатами. [Удаление EnumValue](enum-value-deletion.md)
проверяет поддержку/freeze владельца и членов, typed/decoded XML зависимости.
[Переименование EnumValue](enum-value-rename.md) сохраняет UUID и проверяет старые
namespace зависимости; [именованный FillValue](enum-fill-value.md) редактируется
одной строкой с прежними QName helpers и проверкой конечной цели.
[Создание Attribute](attribute-creation.md) у Catalog/Document использует закрытый
профиль Name/Synonym/Comment/Type и прежнюю policy с проверкой типов, имён и порядка.
[Создание колонок](column-creation.md) использует принятую табличную часть
Catalog/Document с её UUID, parentage и native namespace context.
[Служебные идентификаторы табличной части](tabular-section-identities.md)
сохраняют четыре UUID без повторных generated names; writer/target lookup
используют единственные metadata Name. Preserved YAML не меняет классификацию
при открытии/миграции; служебные UUID не дают policy authority.
[Создание табличных частей](table-creation.md) принимает новый закрытый subtree
Catalog/Document с явными service UUID и новыми колонками; авторитет даёт принятый
родитель, все UUID/имена/типы проверяются до публикации.
[Удаление реквизитов и колонок](attribute-deletion.md) сохраняет относительный
порядок выживших членов и service UUID таблицы; права и консервативный guard
XML/BSL проверяются до generation. Неизвестный профиль восстанавливает состав.
[Переименование реквизитов/колонок](attribute-rename.md) сохраняет UUID и native
профиль; коллизии проверяются по окончательным именам, прежние XML/BSL упоминания
не переписываются и блокируют publication. Общий Attribute profile используется
обоими structural selectors.
[Удаление табличных частей](table-deletion.md) проверяет полный native subtree,
права каждой принятой колонки и table/column/service dependencies; оставшиеся
identities и координаты сохраняются.
Другие structural profiles и остальные составные домены ещё в работе.
[Запросы в `.sdbl`](sdbl-queries.md) входят в MVP:
EF-23 отвечает за файлы/обратную сборку, EF-27 — за анализ и зависимости.
EF-23 — `DONE` в [принятом профиле](query-acceptance.md), EF-27 — `PLANNED`;
реализации и результаты перечислены ниже.

[Именованные свойства EF-24](readable-layout.md) используют descriptor schema 2
с отдельными native details, общими CST/semantic coordinates и обеими явными
миграциями. Карта путей остаётся версии 3; [legacy annotations](annotation-migration.md)
переносятся по concrete schema paths. [Описание Type](type-description.md)
добавляет alternatives и qualifiers с native bindings и общими edits/origins.
[Ссылки метаданных](metadata-reference.md) добавляют одиночные значения и списки,
проверку целей по объявленному дереву, CST-правки и восстановление запрещённых
raw edits до проверки итогового состава.
[Цели Type/TypeSet](type-targets.md) проверяются по native GeneratedType и
действительному URI namespace; lookup общий со ссылками метаданных.
[Именованные наборы](named-type-sets.md) проверяются по native category:
DefinedType не принимает вложенные DefinedType/Characteristic с обеими записями
alternative; ESKA не нормализует допустимые native element names.
Остальные составные домены и structural operations ещё нужны.

[Основа анализа EF-17](analysis-kernel.md) выбрана по реальному API,
несохранённым YAML/BSL, лицензиям и измерениям C1/C2/C3. Проверенный prototype
не является готовым IDE-сервером или полным анализом конфигурации.
[Первый вход EF-18](canonical-inputs.md) проверяет адресное обновление готового
объекта в Salsa и отказ от XML-fallback; этап остаётся незавершённым.
[Общий контроллер ввода](analysis-inputs.md) связывает saved/overlay, revisions,
ошибки, локальные лимиты и отмену без платформы или IDE runtime.
[Входы четырёх семейств](canonical-families.md) добавляют регистры, общие модули
и определяемые типы; полное преобразование YAML пока не реализовано.
[Первый адаптер](../../../tools/bsl-adapter/README.md) напрямую преобразует
общий модуль; [результат проверок](common-module-adapter-results.json) включает
равенство всех 514 моделей C1 с чтением исходного XML.
[Адаптер типов](defined-type-adapter-results.json) проверен на всех
50/508/531 определяемых типах C1/C2/C3 и на несохранённых значениях YAML.
[Адаптер регистров](register-adapter.md) проверяет поля и режимы;
[локальное ядро](kernel-integration.md) добавляет квартальную/годовую
периодичности и сохраняет канонический ввод четырёх семейств.
[Константы и перечисления](primitive-adapter.md) сохраняют типы, реальные UUID
и порядок значений; saved/overlay используют общую модель без XML-экспорта.
[Объекты и табличные части](object-adapter.md) добавляют справочники, документы,
встроенные обработки/отчёты, стандартные реквизиты и ссылки регистров движений.
[Процессы, задачи и планы обмена](analyzer-process-objects.md) расширяют ту же
проекцию и текущую YAML-навигацию, включая реальные адресные реквизиты задач.
[Планы характеристик, счетов и расчёта](analyzer-chart-objects.md) сохраняют
стандартные условия и неявные таблицы, разделяя реальные признаки двух уровней.
[Подробные ошибки моделей](analyzer-adapter-issues.md) передают конкретную причину
и исходное свойство/QName с текущим диапазоном в общем состоянии метаданных.
[Диагностика моделей YAML](analyzer-metadata-diagnostics.md) использует те же причины
и captured координаты, отдельно от BSL rules и без повторного чтения текущего текста.
[Подробный отчёт CI](analyzer-checker-metadata.md) получает findings той же
операцией и использует общую проекцию причин/координат с IDE.
[Семантический профиль checker](semantic-check.md) выполняет настроенные native
rules с загруженными соседними телами. Selected-profile coverage остаётся
отдельной от syntax и платформенной исполнимости.
[Семантический LSP pull](analyzer-semantic-diagnostics.md) использует тот же
набор правил с текущими YAML/BSL inputs; syntax сохранён по умолчанию.
[Подписки на события](analyzer-event-subscriptions.md) передают текущие
Source/Event/Handler в каноническое ядро; IDE и checker используют одну проекцию.
[Регламентные задания](analyzer-scheduled-jobs.md) согласуют current MethodName/
Predefined/Use и тот же name listing; расписание не входит в BSL-модель ядра.
[Подсистемы](analyzer-subsystems.md) передают Content и прямые дочерние имена
главного descriptor; вложенные descriptors сохраняют явную неполноту coverage.
[HTTP-сервисы](analyzer-http-services.md) согласуют current маршруты/Handler
и name/file lookup; модуль BSL загружается независимо от YAML-модели.
[Web-сервисы](analyzer-web-services.md) согласуют Namespace, операции и имена
параметров с тем же name/file lifecycle; XDTO semantics в native модель не входят.
[Сервисы интеграции](analyzer-integration-services.md) передают current каналы и
receive handlers; bindings используются в UnusedLocalMethod.
[Автодополнение сервисов интеграции](analyzer-integration-completion.md) использует
native root/context checks и те же объявленные identities без чтения каналов.
[Адаптер роли](role-adapter.md) соединяет YAML identity с явно переданными
правами сохранённого ресурса; canonical resource lifecycle пока не подключён.
[Reader прав](role-rights-reader.md) даёт общую declared identity и bounded
source bytes с явными missing/unsafe/encoding/size failures.
[Отдельный сервер метаданных](analyzer-metadata.md) согласует экспериментальный
протокол; [синхронизация YAML/BSL](analyzer-documents.md) использует общую сессию,
редакторские версии и квоту. [Pull-диагностика](analyzer-diagnostics.md) проверяет
текущий BSL выбранным профилем `syntax` или `semantic`, как checker.
[Отмена запросов](analyzer-cancellation.md)
передаётся ядру отдельным reader без второго project host. [Лимит времени](analyzer-deadline.md)
использует общий таймер server/checker и сохраняет причину остановки.
[Автодополнение](analyzer-completion.md) получает кандидатов из текущего
YAML/BSL, учитывает возможности клиента и сообщает неполноту.
[Структура текущего BSL](analyzer-document-symbols.md) подключена к стандартному
documentSymbol: negotiated hierarchy/flat, текущие byte/UTF-16 ranges и общий
request control, без второго parser или metadata IO.
[Карточка символа BSL](analyzer-symbol-hover.md) использует структурированный
позиционный API ядра. Hover согласует plaintext/markdown, локализует заголовки
в IDE и сохраняет текущие caller ranges/hash при смене BSL/YAML.
[Параметры текущего вызова](analyzer-call-signatures.md) используют native call
binding того же ядра. SignatureHelp согласует документацию/UTF-16 labels,
выбор перегрузки и параметра, сохраняя текущие caller bytes и явную неполноту.
[Использования BSL-символа](analyzer-references.md) получены типизированным native
поиском. Locations сохраняют текущие target ranges/hash, kind, coverage и caps;
разрешённый пустой поиск отделён от неполного и неподдержанного.
[Переименование локальных BSL-символов](analyzer-local-rename.md) ограничено
методом открытого модуля. Планы связывают native occurrences, editor version/hash
и принятую module policy; capture/неполный поиск дают отказ, файл не записывается.
[Отчёт памяти сессии](analyzer-kernel-memory.md) предоставляет прежние native
table estimates через общий сервер без queries и lazy I/O. Source quota,
неполный heap estimate и недоступный RSS остаются раздельными.
[Освобождение прежнего разбора BSL](kernel-parse-release.md) устраняет удержание
native reparse hint у недоступного ввода. Release и отказ по квоте используют
общую точку ядра; соседние hints и current reload сохраняются. Измерение на
128 крупных модулях каждого состава отделяет RSS от неполных Salsa estimates.
[Очистка вычисленных текстов](kernel-source-sweep.md) сокращает их batch LRU
вместе с parse window. На тех же 128 телах диагностика и hashes совпали,
числовая оценка удерживаемого текста уменьшилась; общий RSS budget ещё не готов.
[Координаты объектов и свойств YAML](analyzer-metadata-locations.md) вычисляются
из текущего разбора; metadata locate выбирает UUID/QName без чтения файла.
[Переход из BSL к объекту YAML](analyzer-metadata-definitions.md) использует
тип коллекции, текущую каноническую модель и те же координаты владельца.
[Переход к дочерним метаданным](analyzer-member-definitions.md) добавляет typed
реквизиты, таблицы/колонки и значения перечислений с точным parent ID.
[События сохранённых файлов](analyzer-watched-files.md) используют общую
инвалидацию YAML/BSL без I/O и приоритет editor authority; клиента watcher ещё нет.
[Изменения состава и настроек](analyzer-project-changes.md) завершают прежний host
с `eska/restartRequired`, явным списком control URI и отказом до нового процесса.
[Регистрация control watcher](analyzer-watcher-registration.md) использует одну
post-initialized LSP-подписку и явное подтверждение клиента.
[Регистрация saved inputs](analyzer-saved-watcher.md) добавляет source-relative
шаблоны в тот же запрос, ограничивает пакеты размером frame и сохраняет readable
изменения при посторонних или двоичных совпадениях. Настоящий client/OS watcher
и автоматический restart расширения ещё не приняты.
[Поля регистров](analyzer-register-definitions.md) связывают typed запись и
измерения отбора с теми же координатами текущего YAML.
[Пространства общих модулей](analyzer-namespace-definitions.md) открывают текущий
owner YAML без чтения target BSL и сохраняют приоритет локальных объявлений.
[Составные типы](analyzer-union-definitions.md) подтверждают YAML-цель только
при одном общем объявлении поля всех содержательных вариантов.
[Планы изменения свойств](analyzer-property-plans.md) связывают текущую editor
version/hash, поддержку из независимо закреплённой базы и общий CST-редактор.
[Полная проверка базы](analyzer-property-cancellation.md) уже поддерживает общий
контроль отмены и времени; адресная policy verification остаётся в работе.
Языковой сервер ещё не завершён.

EF-01 фиксирует покрытие первой версии, а не готовность конвертера.
Профиль — обычная конфигурация Designer XML 2.20. Установленная и проверенная
через `eska platform list --format json` платформа — 8.3.27.2325.

[Покрытие](coverage.md), [машинная матрица](coverage.json) и
[связь требований с этапами](requirements.json) задают входы EF-02–EF-09.
[Результат EF-01](ef-01-results.md) содержит фактически выполненные проверки.
[Схема EF-02](schema.md) описывает реализованный внутренний descriptor и
[проверяемый пример](configuration.example.yaml).
[Импорт EF-03](import.md) создаёт полный ESKA-снимок C1;
[экспорт EF-04](export.md) возвращает полный XML-снимок с проверкой identity,
состава и повторяемости. [Принятая база EF-05](baseline.md) сохраняет полные
исходники и вычисляет разрешения из закреплённой политики.
[Защищённый экспорт EF-06](protected-export.md) объединяет разрешённые правки
с принятым составом и публикует завершённые поколения.
[Точечный редактор EF-07](edit.md) меняет существующее разрешённое свойство
с сохранением оформления и проверкой конфликтов записи.
[Платформенная приёмка EF-08](ef-08-results.md) проверила независимые циклы CF
без правок и после разрешённой правки свойства/BSL с восстановлением запрета.
Применение к БД и запуск клиента не приняты.
[Замеры EF-09](ef-09-results.md) и [методика](performance-protocol.md) сравнивают
полную стоимость ESKA, базы, XML-поколений и Git, отдельно от платформенной приёмки.
[Общая граница EF-10](core-boundary.md) выделяет source operations из зависимостей
действующего приложения.
[Ленивый читатель EF-10](source-reader.md) читает владельца по запросу и даёт
координаты типизированных объявлений в точном снимке YAML.
[Общая проекция EF-10](semantic-projection.md) сохраняет существующие
логические объекты, свойства и ссылки для обоих источников.
[Сборочная библиотека EF-10](build-boundary.md) выполняет нативный конвейер
без приложения и Git; независимый consumer проверен на C1 и установленной 1С.
[Проверенные входы EF-10](prepared-inputs.md) связывают готовое поколение с
private proof и проверяют фактический snapshot до запуска платформы.
[Карта файлов EF-10](source-map.md) записывает выбранные рабочие/принятые
BSL и ресурсы, составное происхождение метаданных и контрольную сумму в manifest.
[Сопоставление координат EF-10](source-locations.md) связывает native byte span
с выбранным YAML/BSL input после проверки точных байтов.
[Адреса сообщений 1С EF-10](platform-locations.md) сопоставляются с картой
проверенного snapshot в сборочном callback; raw output сохраняется.
На установленной 1С проверены предупреждения и ошибка чтения XDTO ресурса;
при отказе CF/manifest не публикуются. Эти сообщения содержат только путь файла.
[Выбор источников](project-selection.md) добавляет явные project settings,
workspace selection и materialization по отдельно переданному pin; XML-only
операции отказывают для ESKA до чтения оставшихся дескрипторов.
[Перенос через Git](git-transport.md) сохраняет байты source/base через scoped
attributes и общий writer; эффективная Git policy остаётся границей VCS.
[Самостоятельный project CLI](project-cli.md) импортирует проект, выбирает member,
читает настройки и готовит защищённый XML без оболочки, Git и платформы.
[Самостоятельный build CLI](build-cli.md) собирает проверенные входы, сохраняет
origins сообщений платформы и публикует artifact/manifest без Git и анализатора.
Небольшой обезличенный корпус находится в
[`tests/fixtures/eska-format/designer`](../../../tests/fixtures/eska-format/designer).
Это модельные fixtures: успешное чтение не подтверждает сборку платформой.

Реальные исходники остаются в соседнем playground. C1 — небольшая конфигурация
со смешанной поддержкой двух поставщиков; C2/C3 — большие стенды чтения и замеров.
Расширение и внешняя обработка используются для проверки отказа неподдержанного
профиля. Имеющийся пустой demo отчёта не считается внешним отчётом для приёмки.

`scripts/collect_corpus.py` читает только указанные корни. Полный manifest,
контрольные суммы, имена объектов и параметры запуска записываются в новый
собственный каталог playground и не включаются в публичный репозиторий.
Нельзя использовать существующий demo как каталог результата.

```sh
python3 docs/research/eska-format/scripts/collect_corpus.py \
  --input C1=../eska-playground/temp_configuration/src \
  --input C2=../eska-playground/temp_accounting_configuration/src
```

Сборка и повторная выгрузка выполняются через `eska` на отдельных копиях C1
в EF-08. Большие конфигурации не собираются. Для каждого цикла фиксируются
версия платформы, режим совместимости и сравнение с контрольным циклом XML.
Синтетические, файловые и платформенные проверки описываются раздельно.

Публичный корпус включает вложенные реквизиты и табличную часть, локальный QName,
неизвестную структуру, nil/пустое/отсутствующее значение, точное длинное число,
смысловой порядок, двоичный ресурс, BSL с BOM/CRLF, файлы вне состава и модельные
правила смешанной поддержки. Недопустимые варианты создаются тестами в собственных
каталогах, не изменением fixtures: коллизии UUID/имён, symlink, escape пути,
повреждённая поддержка, неизвестный XML-профиль и неоднозначный состав.
