# Общая сессия анализа ESKA

Библиотека `eska-analysis` связывает общий контроллер saved/overlay, адаптер
метаданных и локальное ядро BSL. Она находится в отдельном workspace; минимальная
проектная библиотека не зависит от анализатора. Платформа, Git, IDE-сервер и
XML-экспорт не участвуют в чтении.

`eska-check --capabilities --format json` и `eska-analyzer --capabilities --format json`
проверяют совместимость бинарников без project discovery. SDK package version
и linked `source_core_version` сообщаются отдельно; доступный профиль не означает
полноту анализа. В обычном analyzer режиме stdout остаётся framed JSON-RPC.

[Declared `.sdbl` syntax](../../docs/research/eska-format/analyzer-query-syntax.md)
использует те же revision/source quota и native QueryParseError в IDE/checker.
Несохранённые queries требуют явного negotiation; metadata/consumer completeness
остаётся false.
[Контекстная диагностика SDBL](../../docs/research/eska-format/analyzer-query-metadata.md)
имеет отдельный `queryAnalysis` opt-in: native rules используют текущие видимые
CF/CFE metadata без BSL bodies. Старый syntax route сохранён; parameters,
consumer и semantic completeness остаются false. Semantic checker использует
тот же query API с отдельным current context, сохраняя incomplete/exit 2.

[Навигация и права чтения](../../docs/research/eska-format/analyzer-navigation.md)
используют тот же host: project context, declared owners/modules/queries и
bounded policy requests с независимым accepted pin. Readonly capability не
включает edits и не заменяет текущие `read`/`locate` payloads.

[LocalSyntax CFE/EPF/ERF](../../docs/research/eska-format/analyzer-local-syntax.md)
добавляет parse diagnostics и document symbols в общую сессию без canonical
Configuration. Scope явно сообщается, model state — SourceOnly, semantic requests
отказывают до lazy reads. CF сохраняет прежний canonical route; complete syntax
не подменяет complete semantic model и LocalSyntax batch остаётся exit 2.

Canonical CF header сохраняет UUID и имя проверенного source descriptor через
`Configuration::with_identity`. Повторное открытие не создаёт новую идентичность;
неверный current root не восстанавливается из прежнего или native XML значения.

[Own context EPF/ERF](../../docs/research/eska-format/external-own-context.md)
передаёт real external metadata и form underlying owner без Configuration.
Default SDK `open`/`from_reader` выбирают `CanonicalExternal` по проверенному
profile; CLI `--profile semantic` даёт own completion/hover/definition и metadata
YAML diagnostics. Full BSL diagnostics/checker без базы отказывают с
`analysis_configuration_context_required`, checker exit 3. Context completeness
остаётся false. CF/CFE используют отдельный явно объявленный контекст ниже.

[CF/CFE composition](../../docs/research/eska-format/analyzer-cf-cfe-composition.md)
подключена к CLI и SDK: существующий `bsl-analyzer.toml` задаёт base и structured
extensions/dependencies, paths разрешаются относительно выбранного `Project.root`.
Одна база и общий allocator различают одинаковые paths разных native roots.
Семантика подготавливает все текущие видимые вклады; invalid/pending input
отказывает без прежней базы. Общие revision/quota охватывают foreign overlays;
controls включают фактические foreign workspaces и все import receipts.
`SourcePath` и language caller URI сохраняют selected-root facade, targets и
root-qualified input APIs содержат owning root. `composition.current_complete`
не означает полноту query consumers или всей семантики. Standalone CFE сохраняет
LocalSyntax; EPF/ERF остаются в собственном контексте. SDK default/all-features —
184/464 tests, native reference filters — 90; проекты синтетические, framed
transport настоящий, новая платформа/installed Explorer здесь не проверялись.
Экспериментальные root APIs описаны в связанном контракте; admission требует
`analyzer-cli` или `check-cli`, базовые identities доступны default library.

[Language, Template, CommonPicture и CommandGroup](../../docs/research/eska-format/structural-metadata-coverage.md)
валидируются как StructureOnly: проверены текущие descriptor fields, canonical
объекта и DCS/query проверки нет. Покрытие selected BSL rules сохраняет отдельный
ограниченный `semantic_profile_complete`; `semantic_complete` остаётся false.

Состав берётся из проверенного manifest и configuration snapshot. При открытии
все остальные YAML остаются pending, а их идентичности и slot стабильны.
Сохранённые файлы и черновики используют одни преобразователи. Неверный текущий
ввод очищает канонический slot; прежняя модель не используется как fallback.
Результат kernel resolve и координаты имеют явную ревизию. Общий `SessionRevision`
учитывает каждое принятое изменение YAML, ресурса метаданных и BSL, включая неудачное чтение;
внешний потребитель должен дополнять его идентичностью сессии после reopen.

Подключены главные дескрипторы реализованных адаптеров и вложенные формы с XML dialog. Полнота чтения,
полнота главных моделей и покрытие всех вложенных документов различаются.

[Читаемые свойства EF-24](../../docs/research/eska-format/readable-layout.md)
дают ту же lossless/semantic модель из descriptor schema 2. Проверены RU/EN
common completion, unsaved named flags и переход к физическому BSL input;
protocol и native kernel conventions не изменены.

[Раздельное расположение EF-24](../../docs/research/eska-format/descriptor-layout.md)
поддерживается тем же reader/controller. Самостоятельный `form.yaml` получает
один canonical slot с собственной зависимостью на dialog, без второго slot
у родителя. Invalid/overlay формы меняет её текущий контекст BSL; модели родителя
и соседей сохраняются. Native identities модулей остаются прежними.
[Payload layout 3](../../docs/research/eska-format/resource-layout.md) использует
физические paths для чтения, editor buffers и navigation; виртуальные native
paths остаются у ядра для module/form conventions. Common/service backlinks
и current definitions сохраняют объявленные идентификаторы файлов без disk fallback.

[Текущие предопределённые данные](../../docs/research/eska-format/analyzer-predefined.md)
связывают declared ресурс и YAML владельца через общий controller. Их items
публикуются в главном `MetadataObject` и меняют native completion. Неверный XML
очищает этот slot без disk fallback; сосед и nested forms сохраняют свои memos.

[Адресная подготовка completion](../../docs/research/eska-format/analyzer-targeted-completion.md)
загружает YAML своего модуля, его primary resource/текущий dialog и прямой manager
target, выбранный native ядром. Предварительный metadata read для такого доступа
не требуется. [Объявленный manager body](../../docs/research/eska-format/analyzer-manager-completion.md)
также загружается адресно для текущих exports. Остальные bodies и соседние dialogs
остаются pending; ready/editor inputs не перечитываются.
Read-only `module_completions` сохраняет отдельный контракт. Для явно настроенной
CF/CFE composition подготовка сначала загружает все текущие видимые вклады;
обещание адресного чтения single-root route на неё не переносится.
[Прямой common-module namespace](../../docs/research/eska-format/analyzer-common-module-completion.md)
читает owner через прежний frozen common-module index; current native flags
разрешают или запрещают body preparation. Shadowing не загружает чужой namespace.

Для [общих форм](../../docs/research/eska-format/analyzer-forms.md) текущие YAML
и declared dialog публикуют canonical slot в контекст BSL-модуля.
Непредоставленные формы canonical корня больше не читают preserved XML с диска.
`metadata` возвращает текущий YAML-снимок для дерева и свойств, включая виды,
для которых BSL-проекция ещё не поддержана. Объявленные текстовые модули проходят
saved/overlay/close; diagnostics и completions используют те же байты и
канонические метаданные. Бинарные и непрочитанные тела имеют явное состояние.
Лимит текущих загруженных текстов — 64 MiB по умолчанию; это не бюджет RSS.
Общая [квота текущих входов](../../docs/research/eska-format/source-budget.md)
считает исходный размер готовых YAML-моделей, declared rights/dialog/predefined и BSL-текстов,
по умолчанию 256 MiB.
Замены переиспользуют свой резерв; недостаточная квота очищает прежний вход.
`kernel_memory()` возвращает отдельный снимок таблиц Salsa: число instances,
размеры полей и optional heap estimate. `entries` включает memo с вытесненным
значением; нулевая оценка также может означать отсутствие сохранённого значения.
Aggregate может пропускать неинструментированные values и повторно учитывать
общие Arc; сумма не включает всю память процесса.
Снятие статистики не вычисляет запросы и не меняет текущую ревизию.
[`eska/analysis/memory`](../../docs/research/eska-format/analyzer-kernel-memory.md)
передаёт эти оценки IDE-клиенту с обязательным текущим snapshot, отдельно от
source-byte квоты и недоступного RSS. Имена таблиц могут повторяться;
массив сохраняет все таблицы и однозначный порядок по их полным значениям.
[Недоступный BSL](../../docs/research/eska-format/kernel-parse-release.md)
освобождает прежний native reparse hint через общий setter ядра. Release и
отказ по квоте больше не держат старое дерево вне Salsa; другой файл и
повторная загрузка current bytes сохраняют прежнее поведение.
[Пакетная очистка](../../docs/research/eska-format/kernel-source-sweep.md)
сокращает также окна вычисленных source-text и diagnostic memos через штатный
`ide::sweep_lru_deep`; текущие resident inputs, версии и editor hints сохраняются.
Это часть существующей LRU policy, не общий лимит RSS.
`QueryCancellation` прерывает выполняющийся запрос на ближайшем checkpoint
Salsa; один токен управляет одним запросом и не меняет исходники или ревизию.
Самостоятельные checker и IDE transport используют профили `syntax` и `semantic`
этой же сессии с кооперативным deadline. Общий бюджет памяти, полные модели
и семантика ещё не реализованы.
Библиотека не опубликована и не входит в установленный комплект.

Read-only пример `project_session` принимает каталог исходников ESKA, читает
только объявленных владельцев и проверяет результат каждого kernel resolver
точным равенством с текущей моделью. JSON раздельно показывает полноту чтения,
главных моделей и вложенных дескрипторов; неполное покрытие даёт exit 2.
Запуск не создаёт XML и не требует установленных Git/платформы/IDE.

Пример `project_module` читает метаданные и один объявленный общий модуль,
сравнивает syntax-only диагностику saved/editor/close с отдельной batch-сессией.
Неполнота метаданных или модулей остаётся явной и даёт exit 2; пример не является
публичной командой проверки проекта. Экспериментальные методы сессии теперь
принимают `SessionRevision` вместо внутреннего clock контроллера метаданных.

```bash
ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test --manifest-path tools/analysis-core/Cargo.toml --locked
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_session -- /path/to/eska/source
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_module -- /path/to/eska/source
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_memory -- /path/to/eska/source
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_roles -- /path/to/eska/source /path/to/native/source
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_forms -- /path/to/eska/source /path/to/native/source
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_nested_forms -- /path/to/eska/source /path/to/native/source
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_predefined -- /path/to/eska/source /path/to/native/source
```

Команды выполняются из корня `eska`; для тестов путь задаётся к соседнему
playground. Подробности и реальные замеры —
в [результатах сессии](../../docs/research/eska-format/shared-session.md)
и [проверке BSL](../../docs/research/eska-format/bsl-session.md).
Контракт отмены — в [проверке запросов](../../docs/research/eska-format/query-cancellation.md).
Пример `project_memory` читает все YAML и до 40 объявленных текстовых модулей;
сравнивает оценки до/после освобождения текста и глубокой LRU-очистки.
[Реальный замер](../../docs/research/eska-format/kernel-memory.md) показывает
ограничения этих оценок; очистка не гарантирует освобождение всех memo или RSS.

## Самостоятельная проверка

Feature `check-cli` собирает `eska-check` в этом же отдельном workspace.
Проверка читает выбранный проект ESKA и `bsl-analyzer.toml`, использует те же
BSL diagnostics и метаданные, что сессия IDE. Объявленные модули вложенных команд
привязаны к YAML владельца, без отдельного command XML. Необъявленные ресурсы
не становятся активными модулями при обходе каталога.

```bash
cargo build --manifest-path tools/analysis-core/Cargo.toml --features check-cli --release --locked --bin eska-check
```

В каталоге выбранного проекта запускается собранный бинарник:

```bash
eska-check --profile syntax --format json
eska-check --profile syntax --project main --timeout-seconds 300
eska-check --profile syntax --max-retained-source-bytes 268435456
eska-check --profile semantic --format json
```

Его пока нет в установленном комплекте `eska`; путь к собранному бинарнику
нужно указать самостоятельно. Exit codes: 0 — проверенный профиль без находок,
1 — находки выбранного профиля, 2 — неполная проверка, 3 — ошибка запуска или настроек.
Неполнота метаданных не скрывается даже при отсутствии syntax findings.
Подробные [причины ошибок YAML](../../docs/research/eska-format/analyzer-checker-metadata.md)
совпадают с IDE: общий core возвращает typed findings, общий presentation —
machine cause, severity и RU/EN объяснение. JSON schema 1 добавляет `findings`
у metadata issues и отдельное `metadata_position_encoding=utf-16`; прежние BSL
координаты и exit codes сохраняются. Недоступный YAML имеет `findings=null`.
Профиль `syntax` проверяет `ParseError` и освобождает тело после независимого query.
[`semantic`](../../docs/research/eska-format/semantic-check.md) выполняет настроенные
native rules, загрузив все доступные объявленные тела до первого query. Общая
квота текущих YAML/BSL по умолчанию 256 MiB ограничивает и загруженные тела этого
профиля; per-document предел остаётся 64 MiB. Batch trim сохраняет input и
редакторские reparse hints, освобождая saved hints. Это не общий heap/RSS budget.
`semantic_profile_complete` появляется только у semantic и отражает выбранные
правила и покрытие входов; платформенную исполнимость оно не подтверждает.
Human semantic findings содержат rule code, severity и позиции; summary и причины
YAML локализованы RU/EN. IDE использует те же правила при явном
[`--profile semantic`](../../docs/research/eska-format/analyzer-semantic-diagnostics.md).
[Общий контракт](../../docs/research/eska-format/check-cli.md).

## IDE transport метаданных

Отдельная feature `analyzer-cli` собирает `eska-analyzer`. Feature `check-cli`
не включает сервер; библиотека без обеих features работает без транспорта и CLI.
Бинарник использует общий bounded framing/JSON parser, не прежний handshake
приложения. Runtime не зависит от LSP crates, Git, платформы или отладчика.
Внутри транспорта `server.rs` управляет жизненным циклом соединения и одним
worker; `server/initialize.rs` согласует возможности, `server/requests.rs`
проверяет snapshot и передаёт запросы профильным обработчикам. Сессия и порядок
изменений остаются общими.

```bash
cargo build --manifest-path tools/analysis-core/Cargo.toml --features analyzer-cli --release --locked --bin eska-analyzer
```

В выбранном проекте запускается собранный бинарник:

```bash
eska-analyzer --project-dir .
eska-analyzer --project-dir . --profile semantic
eska-analyzer --project-dir . --project main
eska-analyzer --project-dir . --timeout-seconds 300
eska-analyzer --project-dir . --accepted-base-sha256 <approved-base-sha256>
```

Стандартный initialize должен явно запросить ESKA protocol `0.1`; после
initialized доступны страницы владельцев и чтение/refresh одного YAML.
Новый UUID сеанса и строковая ревизия обязательны в custom requests.
Объявлена full-document синхронизация YAML/BSL: open/change/save/close используют
общую ревизию и текущую source-byte квоту. Save не заменяет текст редактора;
close перечитывает файл. [Client file events](../../docs/research/eska-format/analyzer-watched-files.md)
инвалидируют сохранённые YAML/BSL без I/O, с общим preflight и приоритетом editor.
[Control file events](../../docs/research/eska-format/analyzer-project-changes.md)
освобождают прежний host с `eska/restartRequired`; до нового процесса все
проектные операции отклоняются. Список control URI включает родительский
workspace manifest и отсутствующие кандидаты settings.
[Динамическая регистрация](../../docs/research/eska-format/analyzer-watcher-registration.md)
запрашивает одну подписку на эти 7–8 control files после initialized, если клиент
поддерживает оба watcher flags. [Saved-input регистрация](../../docs/research/eska-format/analyzer-saved-watcher.md)
включена вторым элементом того же запроса: source-relative шаблоны покрывают
объявленные YAML/BSL и binary modules. Ответ подтверждается eska/watchRegistration.
Согласованные пакеты ограничены 4 MiB frame; unknown source matches игнорируются,
binary остаются нечитаемыми. Manual policy сохраняет предел 200 и строгий отказ.
Отказ регистрации не блокирует host. Интеграция клиента и автоматическое открытие
нового состава ещё не приняты.
[Guard повторного импорта](../../docs/research/eska-format/analyzer-import-guard.md)
проверяется до dispatch и перед отправкой frame без ожидания watcher event.
Старый host завершается с последним отправленным snapshot; несохранённые
документы остаются у клиента для явного replay в новом процессе.
`textDocument/diagnostic` использует выбранный `--profile syntax|semantic`
и `bsl-analyzer.toml`; syntax остаётся default, settings фиксируются при initialize.
Semantic загружает pending saved inputs до query и сохраняет editor authority;
общий `load_semantic_inputs` используется также checker. UTF-16 диапазоны
получены из текущих байтов без чтения диска. Ошибка settings не отключает дерево.
`textDocument/completion` получает кандидатов из того же текущего YAML/BSL,
учитывает client snippet support и явно сообщает неполноту.
[`textDocument/documentSymbol`](../../docs/research/eska-format/analyzer-document-symbols.md)
возвращает структуру текущего BSL из тех же item/region trees. Клиент выбирает
иерархию стандартной capability; остальные получают плоские SymbolInformation.
Диапазоны/hash принадлежат текущим bytes, соседние inputs не читаются.
Это структура одного модуля; metadata tree обслуживается отдельным API.
[`textDocument/hover`](../../docs/research/eska-format/analyzer-symbol-hover.md)
возвращает typed карточку разрешённого BSL-символа из того же текущего ядра.
IDE presentation согласует plaintext/markdown и переводит заголовки RU/EN;
purpose сохраняет язык автора. [Первое наведение на member callee](../../docs/research/eska-format/analyzer-targeted-hover.md)
подготавливает own context и прямой common/manager target через native discovery
и общий controller. `module_symbol_hover` остаётся отдельным read-only query;
non-callee tokens не читают metadata через эту подготовку. Координаты/hash относятся к caller, в том числе
для карточки другого ready модуля. Неполное отсутствие даёт hover_incomplete;
типы локальных переменных и полный native hover пока не подключены.
[`textDocument/signatureHelp`](../../docs/research/eska-format/analyzer-call-signatures.md)
использует native call binding того же ядра: порядок сигнатур и активный аргумент
не вычисляются вторым parser. Согласуются документация, UTF-16 parameter labels
и nullable selection; изменения target BSL/YAML сохраняют caller hash.
Прежняя карточка клиента игнорируется; неполное отсутствие даёт signature_help_incomplete.
[Адресная подготовка](../../docs/research/eska-format/analyzer-targeted-signatures.md)
в первом запросе загружает current own context и прямой common/manager target,
выбранный native callee discovery. Original argument offset и read-only query
сохраняются; non-member calls не читают metadata через эту подготовку.
[`textDocument/references`](../../docs/research/eska-format/analyzer-references.md)
использует typed native search с обязательным includeDeclaration. Каждый Location
сохраняет current target/hash, kind и явные coverage/caps; пустой неполный поиск
даёт references_incomplete. 200 candidate files и 10000 projected locations
не ограничивают память native collection и не доказывают безопасность rename.
[`prepareRename/rename`](../../docs/research/eska-format/analyzer-local-rename.md)
возвращают versioned планы только для параметров и locals метода открытого BSL.
Требуются accepted pin и client documentChanges; native scope, полные resident
occurrences, current version/hash, newName и pinned module permission проверяются
без загрузки соседних inputs. Capture проверяется консервативно по именам всего
модуля, comments/strings не переписываются. 1024 bytes ограничивает newName,
10000 — projected occurrences. Сервер не применяет план и не записывает BSL.
`textDocument/definition` подтверждает BSL-цель из объявленного ready состава;
диапазон и hash относятся к текущему target text. Не найденная цель при неполных
входах даёт navigation_incomplete. Отдельный [metadata locate](../../docs/research/eska-format/analyzer-metadata-locations.md)
находит объект или QName свойства по UUID в текущем готовом YAML, без чтения диска.
Native path вложенного descriptor сохраняется, URI указывает на физического владельца.
[BSL-символ коллекции](../../docs/research/eska-format/analyzer-metadata-definitions.md)
уже разрешается в текущий верхнеуровневый объект готовой канонической модели,
используя тот же metadata locate. Scope definition теперь
ready-bsl-and-canonical-metadata; YAML-цель содержит kind/UUID/native_path/origin
с собственным hash, отдельно от caller BSL.
[Дочерние символы](../../docs/research/eska-format/analyzer-member-definitions.md)
включают typed реквизиты, таблицы/колонки строк и значения перечислений; capability
metadataTargets — typed-objects-and-members.
[Поля записей регистров и измерения отбора](../../docs/research/eska-format/analyzer-register-definitions.md)
также открываются в текущем YAML. Стандартные поля без YAML-объекта не дают цели.
[Пространство общего модуля](../../docs/research/eska-format/analyzer-namespace-definitions.md)
открывает ready owner YAML без чтения его BSL; ближайшие локальные объявления
сохраняют BSL-цели.
[Составной receiver](../../docs/research/eska-format/analyzer-union-definitions.md)
подтверждает одно общее объявление всех содержательных arms; nullable arms
пропускаются, неоднозначные owners и неподдержанные варианты не дают цели.
[Процессы, задачи и планы обмена](../../docs/research/eska-format/analyzer-process-objects.md)
загружаются в тот же `CanonicalMetadata::Object`; typed поля и адресные реквизиты
задач открывают текущий дочерний YAML. Неявные стандартные поля без исходного
объекта не получают диапазон или UUID.
[Планы характеристик, счетов и расчёта](../../docs/research/eska-format/analyzer-chart-objects.md)
используют ту же объектную проекцию. Неявные строки не получают YAML UUID;
реальные признаки счёта и субконто подтверждаются по своим физическим kind/parent.
[Подписки на события](../../docs/research/eska-format/analyzer-event-subscriptions.md)
добавлены в устойчивый listing и `CanonicalMetadata::EventSubscription`.
Текущие Source/Event/Handler из YAML проходят общий saved/editor lifecycle;
invalid input очищает прежнюю модель, BSL completion использует тот же состав.
[Регламентные задания](../../docs/research/eska-format/analyzer-scheduled-jobs.md)
используют `CanonicalMetadata::ScheduledJob`; MethodName/Predefined/Use из current
YAML и имена BSL completions согласованы с тем же host. Расписание в эту BSL-модель
не входит, исходные свойства остаются в общем descriptor.
[Подсистемы](../../docs/research/eska-format/analyzer-subsystems.md) используют
`CanonicalMetadata::Subsystem` для главного descriptor: current Content и имена
прямых детей согласованы с metadata listing. Вложенные descriptors сохраняют
`metadata_descriptors_unmodeled`; готовая главная модель не скрывает неполноту.
[HTTP-сервисы](../../docs/research/eska-format/analyzer-http-services.md) используют
`CanonicalMetadata::HTTPService` для current RootURL, шаблонов URL и методов.
Name/file resolvers согласованы; объявленный BSL body имеет устойчивый backlink,
а его чтение остаётся отдельным действием общей сессии.
[Web-сервисы](../../docs/research/eska-format/analyzer-web-services.md) используют
`CanonicalMetadata::WebService` для Namespace, операций и имён параметров.
Name/file lookup имеет тот же lifecycle; типы параметров и XDTO остаются в общем
descriptor, поскольку native модель хранит только имена.
[Сервисы интеграции](../../docs/research/eska-format/analyzer-integration-services.md)
используют `CanonicalMetadata::IntegrationService` для каналов и receive handlers.
Current binding учитывается в UnusedLocalMethod; saved batch и resident session
согласованы. [Native completion](../../docs/research/eska-format/analyzer-integration-completion.md)
предлагает видимые объявленные имена этой коллекции в RU/EN без загрузки каналов;
локальное затенение и non-code boundaries остаются у языкового ядра.
[Роли](../../docs/research/eska-format/analyzer-roles.md) связывают текущий YAML
с отдельно версионированным declared `Rights.xml`. Правка ресурса меняет только
свою canonical роль, invalid/missing права очищают slot без default/XML fallback.
Full document sync и saved watcher используют ту же сессию и квоту; current
model failure видна в дереве и checker. В событиях ресурс имеет `kind=role_rights`.
[Общие формы](../../docs/research/eska-format/analyzer-forms.md) используют тот же
контроллер ресурсов: `MetadataResourceInput/State/Kind`, `resource_paths`,
`resource_owner`, `metadata_resource`, `load_resource_saved`, `apply_resource_overlay`
и `close_resource_overlay`. Это изменение неопубликованного Rust API заменяет
прежние методы и типы для прав ролей; их JSON-контракт сохраняется.
Dialog имеет `kind=form_dialog` и capability `formDialogSync`; current events
и commands учитываются в native `UnusedLocalMethod`. Неверный XML очищает модель,
сохраняя raw text/hash; ошибка не получает выдуманного диапазона YAML.
Binary/absent dialogs не моделируются в сессии.
[Общий reader nested форм](../../docs/research/eska-format/nested-form-reader.md)
уже сохраняет full descriptor identity и dialog отдельно от enclosing YAML.
[Текущие nested формы](../../docs/research/eska-format/analyzer-nested-forms.md)
получают отдельные canonical slots и используют тот же resource lifecycle.
Пара ревизий YAML/dialog сохраняет модели главного объекта и siblings при
правке одного ресурса. `OwnerModel.forms` и protocol `state.forms` показывают
отдельные states/full native identities; capability — `nestedFormModels`.
`modeled_descriptors` учитывает готовые формы. JSON checker issue добавляет
`forms` только при nested failures; native disk fallback остаётся закрытым.
[Подробная ошибка модели](../../docs/research/eska-format/analyzer-adapter-issues.md)
сохраняет общий code/range и добавляет конкретный machine tag, свойство или QName.
Та же DTO используется в страницах, чтении и уведомлениях текущего YAML.
[Диагностика моделей YAML](../../docs/research/eska-format/analyzer-metadata-diagnostics.md)
подключена к `textDocument/diagnostic`: provider сохраняется при отключённых или
повреждённых BSL settings. Core возвращает typed причины и captured координаты,
transport добавляет сообщения RU/EN; failed input не получает выдуманного диапазона.
Полная семантика ещё не готова. [Планы свойств YAML](../../docs/research/eska-format/analyzer-property-plans.md)
возвращают versioned WorkspaceEdit для ready открытого документа при независимо
закреплённой базе и client documentChanges support. План не записывает файл и
не меняет input; он использует общий редактор project core. Полная проверка базы
выполняется на каждый запрос; [отмена и deadline](../../docs/research/eska-format/analyzer-property-cancellation.md)
останавливают её кооперативно без потери текущего ввода.
[`$/cancelRequest`](../../docs/research/eska-format/analyzer-cancellation.md) отменяет diagnostic/completion/definition/documentSymbol/hover/signatureHelp/references/prepareRename/rename query
и property plan через отдельный input thread; один worker сохраняет владение YAML/BSL и порядок изменений.
Общий `RequestCancellation` заменяет неопубликованный `batch::BatchCancellation`.
Один внутренний [таймер](../../docs/research/eska-format/analyzer-deadline.md) обслуживает
server и checker. Лимит языкового запроса и property plan по умолчанию 300 секунд; timeout pull
diagnostics возвращает `-32802` с `retriggerRequest=false`, timeout completion/definition/documentSymbol/hover/signatureHelp/references/prepareRename/rename/plan —
`-32000` без этого поля; клиентская отмена всех десяти операций — `-32800`.
Из установленного комплекта бинарник не запускается автоматически.
[Протокол, границы и проверка C1/C2/C3](../../docs/research/eska-format/analyzer-metadata.md).
[Синхронизация документов и события состояния](../../docs/research/eska-format/analyzer-documents.md).
[Pull-диагностика и совпадение с checker](../../docs/research/eska-format/analyzer-diagnostics.md).
[Автодополнение, UTF-16 cursor и capabilities](../../docs/research/eska-format/analyzer-completion.md).
[Текущие BSL-цели и неполная навигация](../../docs/research/eska-format/analyzer-definitions.md).
