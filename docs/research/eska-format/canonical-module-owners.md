# Текущие canonical владельцы BSL-модулей

Kernel `module_metadata_query` получает canonical владельца до любого native
loader. Object/manager/register/common/service modules используют объявленный
состав и текущие slots, как прежде module forms. Подключённый canonical root
с pending, missing или ошибочным owner не получает XML fallback либо чужого
одноимённого владельца.

## Владельцы и композиция

Object и register modules сначала проверяют собственный root/kind/name/slot.
Каждый одноимённый вклад видимого dependency closure должен иметь canonical
slot и разрешённое текущее значение. Затем применяется прежний resolver CF/CFE:
база, зависимости, своё расширение. Неподтверждённый вклад блокирует owner;
unrelated siblings не участвуют. Common modules сохраняют native execution
flags, service modules связаны с собственным объявленным body.

EPF/ERF определяются через `RootKind::External` и bare native object-module path.
Владелец остаётся `MetadataObject`; фиктивная Configuration не создаётся.
Названия каталогов `CommonModules`, `HTTPServices` и других native collections
в root path не подменяют external kind. Тип `Объект` формы берётся из настоящего
main attribute текущего canonical form input.

Physical stem external export может отличаться от metadata Name. Bare body
связывается с зарегистрированным main FileId прямого `<stem>.xml` в собственном
SourceRoot/FileSet; logical Name берётся из единственного соответствующего
listing entry. Проверка `config_index.main` исключает namesake и неоднозначные
bindings. Поиска файлов, YAML/XML чтения и fallback по имени каталога нет.
Две новые группы проверили оба kind, dotted stem/`.XML`, pending/wrong slots,
missing/outside/ambiguous main. Canonical suite — 32 passed, native regressions
— 18 passed; scoped check, strict Clippy/Rustdoc, fmt/diff прошли. Runtime
native-origin EPF/ERF с отличающимся stem подтвердил own field completion.

`Configuration::with_identity(uuid, name)` создаёт CF/CFE header с переданным
UUID. Прежний `Configuration::new(name)` сохраняет генерацию нового UUID и
serialized shape. External не использует ни один configuration constructor.
SDK ещё должен подключить exact CF/CFE identity и actual external composition.

## Отсутствующий контекст конфигурации у внешнего объекта

Canonical external root исключён также из целых Configuration: getters
RootDatabase, inventory, HIR warm/merged и SalsaProvider не вызывают directory
loader. Это действует и при pending current slot. Own object/form context
сохраняется; отсутствие базы не подменяется пустой Configuration. Native XML
roots без canonical admission используют прежний loader.

Экспериментальный Rust API `configuration_for_root` теперь возвращает
`Option<Arc<Configuration>>`: `None` для canonical external, `Some` для прежнего
native пути. Низкоуровневый `MetadataDb::load_configuration` не изменён;
canonical consumers обходят его до вызова. SDK admission внешних проектов
проверяется следующей отдельной частью.

Две новые группы проверили оба external kind, current/pending, все перечисленные
consumers и отсутствие вызовов loader. Canonical suite — 30 passed, native
accessor — 1 passed, native CFE/external integration — 18 passed. All-target
check, strict Clippy/Rustdoc, scoped fmt и diff check прошли. Это Rust fixtures,
без нового запуска платформы или установленного analyzer.

## Lifecycle и граница готовности

Slots остаются tracked Salsa inputs. Saved/dirty/current/pending publication
инвалидирует свой owner; соседний memo не пересчитывается без изменения его
dependencies. Все canonical listings должны быть зарегистрированы до запросов.
Изменение состава или topology требует reopen: отсутствие записи во внешнем
registry не представляется отслеживаемым mutable input.

Новые helpers не читают source files, не сканируют каталоги, не создают новый
cache и не меняют startup/project guards. Прежний native XML loader работает
для roots без canonical admission. Source guards и композиция IDE/batch остаются
ответственностью SDK; этот kernel foundation не принимает standalone external
или CFE/base анализ ESKA целиком.

## Проверки

28 canonical tests прошли, включая 9 новых групп: шесть owner families,
EPF/ERF current owners и forms, namesakes, unavailable/wrong/missing inputs,
CFE dependency visibility/inheritance и сохранение соседних memos. Exact identity
test проверяет supplied UUID и прежнюю сериализацию. Три native external/CFE
integration files дали 18 passed; real XML positive control сохранил loader.
Это Rust tests native loader на XML fixtures; платформа и установленный analyzer
этой частью не запускались. Scoped all-target check, strict Clippy/Rustdoc,
fmt и diff checks прошли.
Версии packages, pin kernel и provenance не менялись. Общая SDK матрица после
следующего подключения проверяется отдельно.
