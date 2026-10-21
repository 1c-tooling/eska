# Делегирование общей команды

T96 выполняется по частям. Сейчас `eska` запускает executable владельца:

| Команды общего CLI | Владелец |
|---|---|
| Без команды, `metadata`, `version`, `ide`, `new`, `init`, `doctor` | `eska-project` |
| `build`, `import`, `patch`, `clean`, `platform`, `config` | `eska-build` |
| `check` | `eska-check` |
| `clone`, `status`, `diff`, `history`, `start`, `switch`, `save`, `finish`, `shelve`, `unshelve`, `shelves` | `eska-vcs` |

`new/init`, включая `new --from`, выполняет project-компонент; он вызывает
build для выбора платформы и распаковки, VCS — для Git-инициализации.
`import` целиком выполняет build-компонент: он заранее
выбирает VCS executable и использует его для обеих проверок changed paths,
включая повторную проверку перед заменой. Git-инициализация `new/init` тоже
вызывает VCS-процесс: его наличие проверяется до файловой транзакции, а
инициализация выполняется внутри неё. `--no-vcs` не требует VCS-компонента.
Существующая Git metadata не переинициализируется; rollback остаётся у ядра.
Перенос остальных частей транзакций
и окончательная очистка зависимостей оболочки ещё не завершены;
T96 остаётся `IN PROGRESS`. `update` сохраняет прежнего владельца до T97.
Планировщик immutable `patch` уже принадлежит VCS: merge-base, committed delta,
допустимые изменения BSL и adopted descriptors используют единственную
реализацию. Baseline и подготовленные модули материализует VCS, native pipeline
принадлежит build runtime, который не открывает Git. Общая команда делегирует
`patch` сборщику; он связывает оба владельца внутренним process-протоколом.
Самостоятельный `eska-build patch` использует тот же путь; даже dry-run требует
VCS-компонент, но не project-сервер, анализатор или установленную платформу.

`eska-project-core::patch::PatchDescription` сохраняет прежние восемь полей
публичного JSON плана. Git OID, repository root, BSL bodies и descriptors
остались приватными данными VCS. Native runtime получает описание и callback
подготовки только своего нового пустого workspace. Повторное заполнение
существующего namespace запрещено. Baseline и opaque bytes копируются точно;
generated XML/BSL сохраняют прежние BOM/CRLF, без дополнительного newline
для однострочного XML. Промежуточный полный набор прошёл: 882 теста (12 ignored),
fmt/check/strict clippy и rustdoc. Fake pipeline подтверждает orchestration,
а не реальную проверку patch платформой.

Экспериментальный Rust `PatchPlan` содержит public `description`, а его Git
и module internals скрыты. `PatchError.code` стал owned `String` для передачи
между владельцами; публичные значения и JSON schema 1 сохранены.

Для переноса `import` прошли 17 прежних CLI-сценариев и 34 проверки build/runtime.
RU/EN preview общего и прямого запуска совпали побайтно в stdout/stderr.
Изолированная установка только build+VCS при пустом PATH проверена для четырёх
типов и обеих локалей: подтверждение, замена, identity, сохранность manifest,
отсутствие новой `.git` и очистка staging. Настоящая 1С этим process-тестом
не подменяется. Экспериментальный Rust `ImportPlan` теперь заимствует callback
с lifetime: обе проверки сохраняют один выбранный VCS executable.

## Создание и подключение проектов

Публичные `eska-project new/init` сохраняют структурный контракт T91, без
автоматического Git и проверки установленной платформы. Прежний интерактивный
контракт общего `eska new/init` обслуживают приватные project-входы
`__shell-new`/`__shell-init`. Оболочка меняет только имя входа; остальные
аргументы, потоки и exit сохраняются. Эти приватные входы не являются новым
публичным CLI-контрактом самостоятельного инструмента.

Для `new --from` сначала проверяются компоненты и платформа. Затем ядро
эксклюзивно создаёт проект. Project владеет transfer-каталогом, build —
соседним native workspace; оба находятся в прежнем игнорируемом `.eska/import`.
После ответа build project заново читает identity из фактического XML,
сверяет её с ответом и публикует через прежний writer/enrollment.
Чужие каталоги и ignore rules не заменяются. Ошибка компонента, повреждённый
ответ или отказ native не оставляют частичный новый проект.

Интерактивный IPC сохраняет stdin и stderr у дочернего владельца, ограничивая
машинный stdout одним MiB. У него нет нового ограничения длительности native
операции. Один cancellation scope живёт всю команду, от выбора платформы до публикации;
Unix-сигнал пересылается только direct child. Ему даётся 30 секунд на cleanup,
после чего действует kill/reap fallback. Владелец временного каталога в ядре
конструируется только эксклюзивным созданием, его путь нельзя подменить.
Финальные проверки флага выполняются внутри владельца файловой транзакции:
после установки source, до enrollment и после его проверки. Если отмена
пришла после записи control files или корневого manifest, прежний rollback
восстанавливает точные исходные байты. У `init` scope не освобождается после
получения BuildSettings. Чистое ядро получает только callback проверки;
OS-сигналы и runtime в его зависимости не добавлены.
Экспериментальные Rust-реэкспорты `create/init` теперь принадлежат project;
`create_imported` принимает `PreparedSources` вместо native `PreparedArtifact`,
а `CreationFailure::Native` хранит отказ процесса. Транзакционные Rust-входы
принимают caller-state/cancellation callbacks. Публичные CLI/JSON сохранены.

[Нативный отчёт onboarding](measurements/t96-onboarding.json) содержит реальные
CF/CFE/EPF/ERF: общий и прямой compatibility-входы прошли `new --from`, `import`
и `init` на 8.3.27.2325 в Distrobox. Identity совпала, project manifests равны,
входы и XML при init сохранены, staging удалён. Реальная отмена вернула exit 1,
завершила owned native PID и удалила только новый проект. Проверенные бинарники
и точные копии скриптов сохраняются в стенде до старта проверок; последующая
пересборка checkout не меняет принятые executable.

Повтор из checkout после `cargo build --workspace --bins --locked`:

```sh
ESKA_TEST_ROOT="$(realpath ../eska-playground)" python3 -B tests/native_onboarding.py \
  --fixtures ../eska-playground/COMPLETED_NATIVE_BUILD_STAND \
  --platform-version 8.3.27.2325 --distrobox YOUR_CONTAINER
```

Fixtures принимает только завершённый собственный стенд `tests/native_build.py`
в соседнем playground и читает его без изменений. Новый стенд создаётся отдельно.
Эта проверка не покрывает Windows/macOS/Remote, host runner, большую `buhmfo`
или работу клиента 1С. Они остаются в приёмке T89.

После проверки поздней отмены прошли полный fmt/check/clippy, 880 тестов (12 ignored) и rustdoc
с `-D warnings`. Отдельно проверены все три Unix-сигнала в RU/EN, отсутствие build,
повреждённый ответ до/после создания каталога и rollback. В первом полном запуске
существующий switch-тест получил `Shelf(Locked)`; отдельно и при полном повторе
он прошёл. Причина этого единичного отказа не установлена, VCS-код не менялся.

## Статическая проверка

`eska check` делегирует `eska-check`, удаляя только имя команды из native argv.
`--project-dir`, пути анализатора/отчёта, stdin/stdout/stderr и статус процесса
сохраняются. Схема аргументов общая и находится в presentation; оболочка
не ссылается на реализацию анализатора. `eska check --help` работает без check.
Проверены полный успех (exit 0), превышение порога (1), неполный поток (3)
в RU/EN с пустым PATH: JSON и stderr прямого и общего входа совпали побайтно,
BSL сохранил BOM/CRLF. Отдельный process fixture подтверждает exit 197 и значение
`--project-dir check`, совпадающее с именем удаляемой команды.

## Диагностика окружения

`eska doctor` делегирует `eska-project doctor`. Чистое ядро проверяет project
source и корневой XML. Project собирает машинные/platform checks у build,
repository/workflow/system Git/LFS — у VCS через скрытые `__bridge doctor`.
Обязательные зависимости обычного project-сервера от Git и платформы
не появились. Порядок прежних checks, JSON schema 1 и exit codes сохранены
при наличии владельцев; RU/EN JSON общего и прямого входа совпал побайтно.

Отсутствующий или сломанный владелец даёт failed check `component.build`
или `component.vcs`, остальные независимые проверки продолжаются. Процесс
с exit 0 и неполным набором обязательных checks не считается успешным.
Human-only детали передаются отдельно от публичного JSON. Все десять прежних
CLI-сценариев прошли, включая точные байты source, index, refs и config после
полного отчёта. Добавлены проверки прямого входа, отсутствующих владельцев
и неполных ответов. Протокол использует обычные лимиты IPC.

Экспериментальный Rust `Check` перенесён в `eska-project-core::doctor` и хранит
owned строки для десериализации. JSON поля и значения не изменены. В прежнем
Rust facade остаются только реэкспорты и композиция единственных реализаций.

## Поиск компонента

`ESKA_PROJECT_BINARY`, `ESKA_BUILD_BINARY`, `ESKA_VCS_BINARY`, `ESKA_CHECK_BINARY` задают явные
пути. Неверный override даёт ошибку и не заменяется другим установленным
бинарником. Без override порядок: рядом с реальным executable `eska`, PATH,
затем `~/.eska/bin` (USERPROFILE на Windows). Имена учитывают `.exe`.
Каталог и сама оболочка не принимаются за отдельный executable компонента.

Отсутствующий обязательный компонент останавливает операцию до проектных
изменений; установщик автоматически не запускается. Справка и версия оболочки
доступны без компонентов. Проверка смешанных версий и manifests установки
завершается в T97. Из checkout перед CLI-регрессиями можно явно собрать все
входы: `cargo build --workspace --bins --locked`.

## Потоки и отмена

`eska-tool-runtime` содержит только поиск компонентов и process/IPC-механику,
без Git, платформы, проектного сервера, анализатора и локализации. Presentation
преобразует структурированные ошибки в RU/EN. Аргументы сохраняют native bytes;
оболочка не использует shell quoting, не декодирует пользовательские streams
и не добавляет сообщения в stdout компонента.

На Unix `CommandExt::exec` заменяет оболочку выбранным компонентом. PID, группа
терминала, stdin/stdout/stderr, TTY и OS-сигналы остаются у той же операции.
Ожидающего wrapper-процесса и polling нет. Именно прежнее ожидание по 10 мс
поймал существующий idle-тест Metadata API; после перехода на exec он прошёл.
На Windows child остаётся в той же консоли; wrapper возвращает его exit code
и ограничивает ожидание после console cancellation десятью секундами.
Полный Windows status вне u8 сохраняется через stable `process::exit` после
завершения child. Windows console delivery, idle и descendants ещё не приняты.

Внутренний одноразовый IPC имеет отдельные piped streams, deadline 30 секунд
и лимит по 1 MiB для stdout/stderr. Проверяются полный JSON, schema 1,
ожидаемая operation, error=null, payload и нулевой exit. Логи передаются
на stderr. Ошибка, переполнение, timeout или отмена не превращаются в пустой
успешный результат. Owned direct child завершается и reaped при отказе.

На Unix IPC и native runner используют совместимые subscriptions `signal-hook`
0.4.5 без default features. Native-флаг принадлежит конкретному `Ibcmd` и его
clones; очистка освобождает только его callbacks. Это устраняет перезапись
IPC-handler прежним `ctrlc` и не добавляет global mutable state на Unix.
Windows сохраняет прежний console-handler. Запросы и native cleanup не ищут
и не завершают процессы по имени.

[Unix exec](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.exec),
[std Child](https://doc.rust-lang.org/std/process/struct.Child.html),
[signal-hook](https://docs.rs/signal-hook/0.4.5/signal_hook/flag/fn.register_usize.html).

## Старые входы проекта

`eska ide --stdio` становится `eska-project serve --legacy-identity --stdio`.
Скрытый flag сохраняет identity `eska` и прежние capabilities; обычный serve
объявляет `eska-project`. Второй сервер или proxy не созданы. EOF останавливает
тот же API runtime. Тихий запуск `eska` без команды использует скрытый
`eska-project __shell-validate` и сохраняет прежний manifest/discovery-контракт.
Эти внутренние входы не появляются в обычной справке project-компонента.

## VCS provenance и preflight

Для `patch` подготовлены отдельные schema-1 запросы:
Build принимает ровно три поля результата plan: public description, fingerprint
из 64 lowercase hex и optional platform version. Public plan проверяется до
путей и native-вызовов. Ответ plan ограничен 64 MiB на stream и 30 секундами;
prepare — 1 MiB на stream и 30 минутами. Другие IPC-запросы сохраняют default.
Непосредственно перед материализацией VCS повторяет планирование и проверку
чистоты, затем сверяет fingerprint. Смена HEAD даёт code=plan-changed.
Build владеет новым пустым workspace и удаляет его после любого отказа;
только прошедший прежние native-проверки кандидат публикуется как новый `.cfe`.

Прямой и общий fake pipeline публикуют один раз и сохраняют прежний artifact
при повторе. Проверена отдельная установка только build+VCS с пустым PATH,
одинаковый RU/EN JSON dry-run, malformed plan/prepare и domain stale-plan
с сохранением source/другого artifact и cleanup. Реальная платформа этим
process-тестом не проверялась.

`__bridge patch-plan [--base REF]` возвращает public `plan`, точную
`platform_version` либо null и `fingerprint`. `__bridge prepare-patch`
принимает `--workspace` и `--expected` (с тем же optional base), заново
проверяет проект, чистоту, endpoints и подготовленные данные, затем заполняет
только пустой workspace. Fingerprint включает metadata, platform, Git blobs,
adopted descriptors и BSL bodies. При несовпадении `plan-changed` возвращается
до записи. Подключение этих запросов к общему CLI ещё не завершено.

Отказ patch имеет exit 1, `data: null` и `error: {code,detail}`; строковые
категории прежних bridge-операций сохранены. Транспорт проверяет обе формы
полностью, ненулевой child exit с partial data не принимается за успех.
Внутренний Rust `Error::Rejected` теперь сохраняет optional detail.
Вызывающий код может задать `QueryPolicy` для конкретной операции; обычные
запросы сохраняют 30 секунд и 1 MiB на stream. Это не меняет API Metadata.

Проверены реальные VCS-процессы с пустым PATH: одинаковые RU/EN планы,
точный public JSON, заполнение workspace, неверный fingerprint и смена HEAD
с отказом до записи. Три transport-теста покрывают domain error/partial reply,
явный лимит большого JSON без ослабления default и timeout с reaping child.

`eska-vcs --project-dir ROOT __bridge provenance` возвращает schema 1,
operation `provenance`, `data` с прежними status/commit/dirty и `error: null`.
`__bridge source-changes --files N [--empty-scaffold]` возвращает operation
`source_changes` и `data.local_changes`. Это скрытые read-only входы композиции.
Files/empty-scaffold передаёт владелец уже проверенного source-плана.

Git-less source сохраняет правило подтверждения (N либо 0 для проверенного
scaffold). Повреждённый Git даёт error=repository и exit 1 для source preflight.
Для provenance тот же случай даёт unavailable, что не доказывает чистоту.
Исходники, HEAD и raw index запросы не меняют. Пути передаются native аргументами.

`eska build --manifest` перед запуском проверяет наличие VCS и передаёт
путь сборщику. `eska-build` запрашивает provenance у владельца в прежней точке
перед snapshot/publication. Протокольный сбой даёт code=vcs-component,
stage=provenance и сохраняет прежнюю пару artifact/manifest. Отмена даёт
interrupted. В dry-run VCS не нужен. Самостоятельный build без выбранного
VCS сохраняет status=unavailable. Для внутренней композиции существует скрытый
`eska-build --vcs-binary PATH`; он не добавляет Git runtime в normal graph build.

На Unix build/import удерживают один флаг отмены также во время discovery
платформы и между IPC-запросами. Проверка `ibcmd --version` не должна сбрасывать
полученный сигнал при начале native-операции. Воспроизведённый регрессионный
тест до исправления завершал import с exit 0 после SIGINT; теперь обе локали
возвращают code=cancelled и exit 1, сохраняя manifest, source и входной файл.
Последующий process-тест обнаружил orphan `ibcmd --version` при отмене doctor:
после завершения build-владельца native PID оставался жив. Теперь discovery
использует общий bounded capture: 30 секунд и 1 MiB на stream, точный kill/reap.
Версия, поиск установок в Distrobox и Designer prerequisite используют эту
механику; длительность native build/unpack pipeline не ограничена этим timeout.
Один scope передаётся через ToolOptions в discovery и в созданный Ibcmd;
полученный сигнал сохраняется между probe и native setup. Artifact selector
тоже сохраняет свой scope в возвращаемом инструменте.

Doctor даёт build-владельцу до 30 секунд на cleanup при отмене или timeout,
пересылая исходный Unix-сигнал (при timeout — TERM). Причина отказа сохраняется,
даже если child успел вернуть exit 0 после cleanup. Прочие IPC-запросы по
умолчанию сохраняют немедленный kill/reap. `QueryPolicy.cancellation_grace`
и raw `capture` — внутренние Rust-входы транспорта, публичный JSON не меняется.

Для Distrobox PID-файл должен быть виден host и контейнеру: он находится в
новом приватном каталоге `~/.cache/eska/probes`, а при изолированном
`ESKA_CONFIG_DIR` — в соседнем `.eska-probes` своего стенда. Каталог имеет mode
0700 и удаляется после reaping. Неудачная запись PID не запускает native-команду.
Исходники и manifests не используются как control-файлы. Host probe отдельного PID-файла
не требует. ToolOptions теперь содержит transient cancellation state и больше
не реализует Eq/PartialEq; экспериментальный Rust API изменён, CLI/JSON сохранены.

Прошли 30 сочетаний doctor/build/import/unpack/platform × RU/EN × INT/TERM/HUP:
probe не освобождался тестом, команда вернула exit 1, PID исчез, исходный XML
сохранился, новый artifact/export не появился. Windows lifecycle пока не принят.

Import запрашивает changed paths при подготовке и повторно перед записью.
Неверный ответ и отказ не дают ложный ноль; отмена сохраняет категорию cancelled,
исходники и prepared staging cleanup. `GitProvenance` находится в общем ядре
как данные; десериализация отклоняет несогласованные поля и неверные IDs.

Экспериментальный Rust API обработчика build принимает fallible provenance
provider. `Ibcmd` больше не имеет Eq/PartialEq: transient cancellation context
принадлежит экземпляру, clones разделяют этот контекст. JSON-схемы не изменены.

## Первые проверки делегирования

Полный workspace fmt/check/strict Clippy/test/Rustdoc прошёл: 872 теста passed,
12 ignored. Сохранены прежние CLI/API/workflow/semantic проверки. Дополнительно
проверены binary streams, native/non-UTF8 args, exit 197, identity rewrite,
неверный override, Unix-сигналы, idle API, timeout с фактическим reaping,
шесть вариантов сломанного provenance и отмена второго preflight после export.

[Нативный отчёт](measurements/t96-native.json): Linux x86_64, Distrobox,
8.3.27.2325, только собственные маленькие CF/CFE/EPF/ERF. Build через оболочку
и повтор напрямую с тем же VCS дали одинаковый JSON после удаления duration_ms
и одинаковые source/project/platform данные паспорта. Проверены checksums,
исходные Name/UUID, повторное использование ИБ, unpack, clean и сохранность
source. Реальные отмены unpack и import-stage build освободили owned PID/staging
и сохранили прежние артефакт, паспорт и managed infobase.

Побайтовое равенство native-артефактов не подтверждено: все четыре типа меняют
checksum и при контрольном повторе «standalone → standalone» с тем же входом.
Оба SHA-256 сохранены; каждый паспорт проверен против своего результата.
Причина различия внутри native-артефактов здесь не установлена.

Повтор матрицы оболочки и прямого входа:

```sh
ESKA_TEST_ROOT="$(realpath ../eska-playground)" python3 tests/native_build.py \
  --binary target/debug/eska-build --shell-binary target/debug/eska \
  --vcs-binary target/debug/eska-vcs --distrobox "$ESKA_DISTROBOX" \
  --ibcmd "$ESKA_IBCMD" --platform-version "$PLATFORM_VERSION"
```

Без shell-binary выполняется контрольный прямой повтор. Параметры платформы
задаёт оператор. Windows/macOS, Remote, настоящий host runner, тяжёлая сборка
buhmfo и функции приложения в клиенте 1С этим результатом не покрыты.

## Повтор после исправления probes

Полный workspace fmt/check/strict Clippy/test/Rustdoc для `5c955ff` прошёл:
896 тестов passed, 12 ignored. [Нативный отчёт](measurements/t96-probe-native.json)
повторяет обе матрицы после нового capture и передачи cancellation scope.
Бинарники не менялись между началом build-матрицы и сохранёнными копиями
onboarding; SHA-256 проверены. Для всех CF/CFE/EPF/ERF прошли общий/прямой build,
reuse, паспорта, unpack/clean и общий/прямой new/import/init. Отмены build,
unpack и создания проекта освободили native PID и staging, сохранили входы,
прежние результаты и managed infobase. Оба control storage пусты после проверки.

Это Linux/Distrobox 8.3.27.2325 на малых собственных fixtures. Побайтовое
равенство повторных native-артефактов по-прежнему не установлено; предыдущий
контроль standalone тоже давал разные байты. Windows/macOS, Remote,
host runner, нативная сборка buhmfo и поведение клиента 1С не проверены.

T96 ещё требует выделения общей чистой схемы CLI и удаления переходных
зависимостей оболочки от исполнителей. Текущее делегирование не означает
завершённую выборочную поставку T97.
