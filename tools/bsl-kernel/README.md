# Закреплённое ядро BSL для ESKA

Этот отдельный Cargo workspace содержит десять изменённых пакетов bsl-analyzer из
ревизии `9021ba07471eee8a2614bed2105a2c8699f60780`. Остальные зависимости на
bsl-analyzer закреплены на той же Git-ревизии. Это исходники библиотек для
экспериментального адаптера, а не опубликованный analyzer/check или LSP-сервер.
Главный workspace ESKA и его минимальный project core от ядра не зависят.

[Параметры сеанса](../../docs/research/eska-format/analyzer-session-parameters.md)
имеют отдельные current slots и typed fields настоящего SessionParameters.
Локальный `hir-def` расширяет прежний ConfigsDatabase двумя per-file методами;
root visibility, adoption UUID, shadowing и platform environment masks сохраняются.
Неизвестное текущее значение не подменяется base/XML fallback.

[Общие реквизиты с явным составом](../../docs/research/eska-format/analyzer-common-attributes.md)
имеют отдельную canonical модель и listing. Per-file object/register resolver
добавляет их настоящие типизированные поля после видимых CF/CFE contributions;
заимствование требует UUID базы. Пока принимаются только явный Content,
AutoUse=DontUse и DataSeparation=DontUse. Pending/invalid contribution не
подменяется сохранённым XML; общий реквизит не получает StructureOnly.

[Общая команда](../../docs/research/eska-format/analyzer-common-commands.md)
передаёт текущий CommandParameterType/ParameterUseMode в отдельный owner
CommandModule. Native inference задаёт тип первого параметра штатной процедуры;
Multiple сохраняет тип элемента массива. UUID-proven inheritance, current failure
и visibility остаются у ядра. Positional symbol cards показывают известные типы
параметров/локальных значений; cold discovery узнаёт один inferred reference owner.

[Параметры функциональных опций](../../docs/research/eska-format/analyzer-functional-option-parameters.md)
сохраняют отдельный canonical listing и UUID-проверенный per-file resolver.
Настоящий interface getter возвращает открытую Structure с возможными ключами
и подсказками типов из текущего Use. Native soft policy чтения полей сохранена;
runtime presence и жёсткий scalar field inference не утверждаются.
Экспериментальные constructors/listing inputs расширены шестнадцатым семейством.

[Последовательности](../../docs/research/eska-format/analyzer-sequences.md)
добавляют семнадцатое canonical family и отдельный record-set module context.
Настоящие generated platform types сохраняют native method IDs и текущие
измерения; Add/Get/iteration и ThisObject используют один тип записи.
Current known generated shape участвует в UnresolvedField, а method cards
и signatures берут типы из того же call binding. Helper нормализации placeholders
справочника общий с FilterCriterion; `bsl-types` не копируется и pin не меняется.

[Хранилища настроек](../../docs/research/eska-format/analyzer-settings-storages.md)
добавляют восемнадцатое canonical family. Native manager methods и текущие
экспортные методы используют один call binding; `Load` сохраняет `Any`.
Cold preparation выбирает объявленные тела по receiver и reaching aliases,
сохраняя видимость, overlays и unread barrier. Предопределённый `ThisObject`
отозван только в проверенных платформой SettingsStorage/Catalog ManagerModule;
пользовательские bindings/globals и qualified manager calls сохраняются.

[Canonical владельцы модулей](../../docs/research/eska-format/canonical-module-owners.md)
разрешаются через текущие slots до native loader; EPF/ERF остаются объектами.
Все listings регистрируются до запросов; новый состав/topology требует reopen.
`Configuration::with_identity` сохраняет фактический UUID CF/CFE.
Whole-Configuration consumers исключают canonical external, включая pending;
экспериментальный `configuration_for_root` возвращает `Option`. Native XML
fallback без canonical admission сохранён.
Native external stem не определяет logical Name: body связан с зарегистрированным
main descriptor FileId; namesake и неоднозначность отвергаются до resolve.
Per-file object/register/defined-type lookup отличает отсутствующую декларацию
от зарегистрированного недоступного canonical вклада: прежняя база не подменяет
его текущее значение. Name-only register lookup соблюдает то же правило.

[CF/CFE references](../../docs/research/eska-format/analyzer-cf-cfe-composition.md)
добавляют `ReferenceArea.candidate_roots`: явные registered native roots расширяют
name-usage candidates с сортировкой/dedup до cap и прежним `files` intersection.
`None` сохраняет anchor-root behavior; semantic target identity и file-local
scope не меняются. Шесть новых и 84 прежних reference tests прошли; SDK —
184 default / 464 all-features tests. Это bounded synthetic/framed приёмка,
а не платформенная проверка всей composition.

[Standalone SDBL](../../docs/research/eska-format/analyzer-query-metadata.md)
использует `FileQueryResolver` с существующим descriptor FileId владельца:
object/register/defined-type lookup сохраняет current per-file visibility.
`ide::sdbl_query_codes` открывает прежний native inventory для явного negotiation
и rule preflight; dispatcher и rules не продублированы. Readiness и source
authority проверяет SDK перед вызовом native query.

Локальный `parser` сохраняет file-leading BOM как native trivia в standalone
SDBL. Исходные байты и ranges не сдвигаются; BOM внутри запроса и повторный BOM
остаются ошибками. Общий converter используется обоими parser entry points,
поэтому syntax diagnostics и output/consumer model согласованы.

Явные типы параметров текущей СКД передаются через `QueryParameterTypes` в
`FileQueryResolver` и прежний lowering локального `sdbl-hir`. Экспериментальный
`Analysis::query_text_model` принимает третьим аргументом этот явный контекст;
пустой контекст сохраняет unknown. Неоднозначность имён не разрешается выбором
первого или последнего объявления; тип одноимённого параметра сеанса не участвует.

Для [адаптера роли](../../docs/research/eska-format/role-adapter.md) открыт
`Role::with_data`: source UUID и current rights передаются явно; serialized
shape и прежний XML reader не изменены.

[Canonical форма](../../docs/research/eska-format/canonical-form-input.md)
передаёт текущий `Form` через slot и существующий BSL module context. Pending,
wrong identity/kind и непредоставленная nested форма в canonical корне не
читают dialog с диска. Прежний native loader для Designer-корней сохраняется;
[producer общих](../../docs/research/eska-format/analyzer-forms.md) и
[вложенных форм](../../docs/research/eska-format/analyzer-nested-forms.md) подключены.

[Предопределённые элементы](../../docs/research/eska-format/analyzer-predefined.md)
используют существующий `MetadataObject.predefined_items` и native completion.
Добавлен `try_parse_predefined_xml` с явным syntax/root failure; legacy tolerant
parser, extraction semantics и Designer callers сохранены.

[Адресный completion](../../docs/research/eska-format/analyzer-targeted-completion.md)
добавляет `Analysis::completion_metadata_target`: native syntax, root ownership
и environment rules определяют прямую MDO dependency без загрузки её payload.
Host готовит текущие inputs; существующий completion query остаётся read-only.
[Методы менеджера](../../docs/research/eska-format/analyzer-manager-completion.md)
добавляют `Analysis::completion_manager_module`: тот же native context и resident
module index выбирают объявленный FileId без чтения BSL. Прежние symbol/signature
queries определяют экспортность и подсказки текущего текста.
[Общий модуль](../../docs/research/eska-format/analyzer-common-module-completion.md)
расширяет owner discovery и добавляет `Analysis::completion_common_module` для
доступного body. Shared positional ownership predicate применяется также к прежнему
fast path: локальное значение больше не получает exports одноимённого namespace.

[Подготовка hover](../../docs/research/eska-format/analyzer-targeted-hover.md)
добавляет `Analysis::symbol_hover_dependency_offset`: текущий called-member token
выбирает свой completion anchor, включая оба края имени. Позиционная карточка
сначала использует native typed method resolution; textual namespace route
проверяет владение root через общий с completion predicate. Локальный receiver
не получает метод одноимённого модуля; его платформенный метод сохраняется.

[Подготовка сигнатуры](../../docs/research/eska-format/analyzer-targeted-signatures.md)
добавляет `Analysis::signature_help_dependency_offset`: nearest active native call
выбирает member anchor его собственного callee. Host использует прежние discovery
и input controller; signature query сохраняет original argument offset и native
call binding. Argument receivers, non-member/inactive calls не подбирают другую цель.

Назначение локальных пакетов:

- `ide-diagnostics`: metadata diagnostics используют общий диапазон первых
  Unicode scalar values файла вместо фиксированного числа байтов. BOM, CRLF,
  кириллица и emoji сохраняют исходные позиции; настоящие AST/method ranges
  и строгая проверка диапазонов адаптера не меняются. Копия того же pinned crate
  включает его build script, descriptions, dictionaries и tests; его прежняя
  лицензия `LGPL-3.0-or-later` и upstream notices сохранены.
- `ide`: [автодополнение сервисов интеграции](../../docs/research/eska-format/analyzer-integration-completion.md)
  использует прежний semantic root claim, context masks и новый getter видимых
  объявленных имён. YAML каналов и BSL bodies целей не читаются. Detail RU/EN
  формируется в native completion; private/unresolved Rustdoc links исправлены.
- `base-db`: недоступный ввод освобождает прежний reparse snapshot после
  exclusive input write. Снимок больше не держит старое BSL-дерево при release,
  пропавшем файле или отказе по квоте. Повторная загрузка и hints других файлов
  сохраняют прежнее поведение; Salsa memos очищаются отдельным механизмом.
  Deep LRU trim временно сокращает окна parse и source-text memos до 64 файлов,
  затем восстанавливает прежние interactive caps 512. Текущий resident input
  сохраняется. Semantic batch дополнительно освобождает saved reparse hints,
  сохраняя hints открытых редакторских документов; это не общий бюджет heap/RSS.
- `ide-db`: отслеживаемые канонические входы объектов, регистров, общих модулей,
  определяемых типов, подписок на события, регламентных заданий, подсистем,
  HTTP-, Web-сервисов, сервисов интеграции и ролей.
  Отдельный slot
  содержит значение и source revision;
  зарегистрированный недоступный или несовместимый slot не читает XML и не
  возвращает прежнюю модель. Прежний XML-путь для остальных входов сохранён.
  HTTP/Web/integration-service lookup по имени и файлу модуля использует тот же tracked resolver;
  зарегистрированный invalid slot не проходит в XML-путь через body backlink.
- `bsl-metadata`: `Quarter`/`Year` в периодичности регистров сведений, нативном
  XML-reader и сериализации. Сохраняются поле `Период` и обе таблицы срезов.
  Для EventSubscription/ScheduledJob добавлены `Eq` и `from_parts`, чтобы публиковать
  готовые модели без XML intermediary; нативные parsers этих видов не менялись.
  Subsystem получает `Eq`; его существующие builders используются без изменения
  XML-parser. Content и прямые дочерние имена публикуются одним tracked значением.
- `hir-ty`: полное сопоставление расширенного enum в вычислении стандартных
  ключей фильтра регистра. Без этого локального пакета прежний exhaustive match
  не собирается с обновлённой моделью.
- `parser-error`: английское представление типизированных ошибок и токенов.
  Для нетипизированного `Custom` используется `Syntax error`: upstream хранит
  русский текст без отдельного кода причины. Русское представление сохранено.
  Общая сессия использует структурированные ошибки уже выполненного parse,
  сохраняя suppression, диапазоны и severity исходной диагностики.

[`eska-bsl-adapter`](../bsl-adapter/README.md) уже использует локальный
`bsl-metadata` через Cargo patch. [`eska-analysis`](../analysis-core/README.md)
применяет локальные patch и закрепление Salsa в собственном корневом manifest:
Cargo не наследует patch зависимости. Контроллер ввода, адаптер и ядро сведены
в общей библиотеке; самостоятельные `eska-analyzer` и `eska-check` подключены
экспериментально. Полная семантика и общий бюджет памяти пока не завершены.

Salsa и salsa-macros закреплены на исходной ветке зависимостей bsl-analyzer:
ревизия `5d2686f1f94dc3d618569652d5576ce997491b3b`, версия `0.28.2`. Она содержит
используемый `ide-db` API оценки памяти. Это закрепление зависимостей, а не
реализованный общий бюджет памяти. Общая сессия поддерживает cooperative
отмену запросов; сервер и checker используют общий RequestCancellation.

[`upstream-provenance.json`](upstream-provenance.json) хранит SHA-256
**оригинальных** файлов до правок. Изменены два manifest для ссылок на workspace,
три файла для периодичности и четыре файла `ide-db` для канонического ввода.
Добавлен `canonical_metadata_tests.rs`; `rustfmt.toml` сохранён из upstream,
чтобы не переформатировать остальной код. Для строгой сборки Rustdoc исправлены
ссылки на приватные и неквалифицированные элементы в шести исходных файлах.
Для `base-db` записаны исходные hashes всех семи файлов той же ревизии;
Production-правки освобождают недоступные snapshots, согласуют sweep caps
parse/source memos и удерживают hints только для заданных editor files.
Добавлены native regression tests; единственный временный fixture создаётся
в ESKA_TEST_ROOT и исправлены
три private links Rustdoc. Существующий `ide-db` комментарий уточняет смысл
entries: они включают metadata-only memos и не считают только живые значения.
В одном BSL-fixture удалена табуляция на пустой строке; BOM и LF сохранены,
диагностика установленного анализатора совпала с исходным образцом.
Поиск тел общих модулей учитывает текущее unread-состояние FileId в устойчивом
listing: pending или удалённое тело не выдаётся за доступный исходник.
Полный набор лицензий, `NOTICE` и
`LICENSING.md` сохранён без изменения. Последний описывает также компоненты
исходного upstream workspace, которые здесь не скопированы. Условия транзитивных
компонентов и платформенных данных сохраняются; готовность к публикации комплекта
этот эксперимент не подтверждает.

[Приёмка и ограничения](../../docs/research/eska-format/kernel-integration.md)
содержат результаты локальных проверок и чтения регистров C1/C2/C3.
[Освобождение parse hints](../../docs/research/eska-format/kernel-parse-release.md)
проверено отдельным сравнением на крупных настоящих модулях этих составов.
[Пакетная очистка source memos](../../docs/research/eska-format/kernel-source-sweep.md)
включает вычисленные тексты в ту же существующую LRU policy.
[Подписки на события](../../docs/research/eska-format/analyzer-event-subscriptions.md)
получают current canonical slot с той же инвалидацией и запретом XML fallback.
[Регламентные задания](../../docs/research/eska-format/analyzer-scheduled-jobs.md)
передают current callback/flags через такой же slot и устойчивый name listing.
[Критерии отбора](../../docs/research/eska-format/analyzer-filter-criteria.md)
используют отдельный текущий slot/listing и реальные generated manager types.
Native `Find` уточняет тип аргумента и массива ссылок; позиционные карточки и
signature help сохраняют native candidates и их документацию.
[Функциональные опции](../../docs/research/eska-format/analyzer-functional-options.md)
имеют отдельный slot/listing и уточняют native `GetFunctionalOption` текущим
storage type, включая возможный `NULL` для реквизита справочника.
Cold discovery и позиционные consumers сохраняют native call ownership;
одноимённая функция модуля получает приоритет перед platform registry.

[Неявные positional locals](../../docs/research/eska-format/analyzer-implicit-local-hover.md)
используют существующие native symbol identity, reaching declaration и тип
конкретного вхождения, включая UTF-16 ranges после смены типа.
