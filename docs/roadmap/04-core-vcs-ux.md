# Основной VCS UX

Все задачи зависят от T07–T11. Human output локализуется; JSON является стабильным
нелокализованным API.

## T12 — `eska status`

**Статус:** `DONE`

Показывать состояние проекта, workflow, task/branch/base, ChangeSet summary,
ahead/behind, locks и readiness к save/publish. Это не копия `git status`.
Обязателен `--format json` со schema tests.

Принятые границы T12:

- task определяется по точному совпадению текущей ветки с task-шаблоном policy;
- ahead/behind считается относительно remote-tracking ref базовой ветки без fetch;
- ChangeSet остаётся file-level и ограничивается корнем проекта внутри worktree;
- отсутствующие remote base и locking представлены как недоступные данные, а не нули;
- JSON schema версии 1 не локализуется и проверяется end-to-end в обеих локалях.

## T13 — `eska start <task>`

**Статус:** `DONE`
**Зависит от:** T12

Выполнять policy plan: fetch/update base, создать и переключить task branch,
зарегистрировать task. Dirty workspace не уничтожать; отказ или явно безопасный
путь. `--kind hotfix` добавлять только вместе с готовой policy.

Принятые границы T13:

- task регистрируется активной веткой по policy, без отдельного state-файла;
- preflight требует чистоты всего worktree и attached HEAD с первым commit;
- при настроенном remote из policy выполняется fetch, локальная base обновляется
  только fast-forward; локальная base впереди remote сохраняется, divergence
  отклоняется;
- отсутствие remote не блокирует локальный старт: fetch пропускается, а task-ветка
  создаётся от локальной base; ошибка доступа к настроенному remote содержит его
  имя, безопасный URL и сохранённую причину Git;
- уже существующая task-ветка не переиспользуется и не перезаписывается;
- системный Git из единого infrastructure layer используется для network/mutating
  операций без разбора human output; Git credentials и transport environment
  сохраняются, repository redirects удаляются;
- `--kind hotfix`, shelve, JSON и отдельное task-state хранилище не добавлены.

**Проверено:** `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test` — успешно
(69 unit, 76 integration). Реальные Git-сценарии покрывают актуальную, отстающую,
опережающую и разошедшуюся base, существующую task-ветку и dirty worktree.
CLI end-to-end проверяет Trunk/Git Flow, RU/EN, exit code, stdout/stderr и
регистрацию task через последующий JSON `status`. Отдельно проверены локальный
старт без remote, подробная ошибка недоступного remote и редактирование пароля
в URL. Ручные Trunk RU и Git Flow EN сценарии выполнены в отдельных временных
проектах playground.

## T14 — `eska diff`

**Статус:** `DONE`
**Зависит от:** T12

Сначала file-level representation. Режимы: human, `--raw`, `--format json`.
Внутренний результат должен допускать последующее object-aware расширение без
изменения назначения команды.

Принятые границы T14:

- команда читает текущие состояния HEAD → index и index → worktree через
  существующий `gix` repository layer, не запускает системный Git и ничего не
  изменяет;
- optional revisions сравнивают локально доступные committed trees: одна revision
  сравнивается с HEAD, две — друг с другом; `--since-branch-point` использует
  merge base, не выполняя fetch и не включая изменения рабочей копии;
- file-level результат ограничен корнем проекта, пути относительно проекта
  отсортированы детерминированно; workflow для diff не требуется;
- human-режим локализован, распознанные пути сгруппированы по типу метаданных и
  внутри типа по точному состоянию; подгруппа содержит количество, а каждая
  identity — одноколоночный state symbol. В TTY цветом выделяются только
  заголовки и символы, `NO_COLOR` и redirect отключают ANSI. Свойства изменённых
  дочерних объектов главного XML-дескриптора уточняются сравнением
  HEAD/index/worktree, а
  нераспознанные файлы сохраняют исходный project-relative путь;
- служебные файлы Designer XML сворачиваются в ближайшего владельца метаданных:
  поддержаны справка, команды, формы, макеты, картинки, XDTO/WS payload,
  бинарные модули, вложенные подсистемы, нумераторы и корневой `Ext` конфигурации;
- raw использует две стабильные колонки состояний,
  JSON schema версии 1 содержит массив файлов с `path`, `path_encoding`, `index`
  и `worktree`, не зависит от locale и не меняется из-за human-проекции;
- revision raw использует одну колонку состояния, а JSON schema версии 2 содержит
  explicit comparison endpoints, strategy, resolved commit IDs и file `change`;
- не-UTF-8 Git-пути не теряются: JSON использует обратимое percent-кодирование,
  human/raw — однострочное escaped-представление;
- patch/hunks, методы BSL, элементы форм и полная semantic Designer XML model не
  входят в эту доработку T14 и остаются в T19–T21.

**Проверено:** `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test` — успешно
(80 unit, 97 integration). CLI end-to-end проверяет RU/EN, чистый проект,
staged/unstaged/untracked, вложенный project scope, отсутствие workflow,
human/raw/JSON, сравнение веток, тегов и commits, merge base, стабильные JSON
schemas, exit codes и ошибки репозитория. Ручные RU human и EN JSON/raw сценарии
выполнены в отдельных временных проектах playground.

## T15 — `eska save`

**Статус:** `DONE`
**Зависит от:** T14

Сохранять точно выбранный ChangeSet. Поддержать `-m`; без него допустим configured
editor. Не заставлять пользователя понимать staging и не анализировать изменения,
которые не войдут в commit. Interactive/auto generation отложены до T22.

Принятые границы T15:

- текущий ChangeSet — все staged, unstaged, deleted и untracked paths внутри
  корня проекта; ignored paths не входят;
- вложенный проект сохраняется отдельно: изменения и staging sibling paths не
  включаются в commit и остаются подготовленными;
- технический staging выполняется системным Git через единый infrastructure
  layer; при ошибке stage, editor, hook или commit исходный index восстанавливается
  байт-в-байт, рабочие файлы не изменяются;
- detached HEAD, пустой ChangeSet, пустое сообщение и конфликты проекта
  отклоняются; первый commit в unborn repository поддерживается;
- `-m` передаёт точное сообщение, без него используется configured Git editor;
  interactive selection, auto generation, JSON и semantic-анализ не добавлены.

**Проверено:** `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test` — успешно
(79 unit, 97 integration). Core-сценарии проверяют первый commit, project scope,
полный worktree content и сохранение sibling staging. CLI end-to-end проверяет
RU/EN, `-m`, configured editor, пустой ChangeSet, detached HEAD, конфликты и
byte-for-byte rollback index после отказа pre-commit hook. Ручные RU/EN сценарии
выполнены в отдельном временном проекте playground.

## T16 — `eska clone`

**Статус:** `DONE`
**Зависит от:** T03, T07

Клонировать существующий готовый проект `eska` через transport `gix`:

```text
eska clone <repository> [directory]
```

Границы первой версии:

- clone, fetch объектов и checkout выполняются через `gix`;
- для pack и checkout включена параллельная обработка `gix`;
- поддерживаются URL и локальный путь, необязательный каталог назначения и
  явное имя remote; по умолчанию remote называется `origin`;
- HTTP(S) не требует system Git; локальный и SSH transport используют внешние
  protocol helpers `git-upload-pack`/`ssh`, как upstream `gix clone`;
- каталог назначения должен отсутствовать: команда не смешивает clone с файлами
  пользователя и не перезаписывает существующий repository;
- после checkout проект проходит обычный discovery и validation `eska.toml`;
  repository без валидного проекта `eska` отклоняется;
- при ошибке удаляется только каталог, созданный текущим запуском;
- `clone` не выполняет `init`, не исправляет чужую конфигурацию и не добавляет
  shallow clone, выбор ветки или submodules без отдельной задачи;
- локальные repositories покрываются integration-тестами; transport,
  credentials, filters/LFS и безопасные diagnostics проверяются соразмерно
  фактически поддержанной матрице.

Решения T16:

- production-путь повторяет upstream `gitoxide-core`: подготовка clone, fetch,
  checkout и проверка outcome выполняются API `gix 0.87.1`;
- `gix/max-performance` включает параллельные pack/index/checkout операции;
  HTTP(S) использует Curl/Rustls без системного OpenSSL;
- destination сначала захватывается эксклюзивным созданием каталога; при любой
  обычной ошибке fetch, checkout или project validation удаляется только он;
- непустые checkout collisions/errors считаются незавершённым checkout, даже
  когда низкоуровневый вызов `gix` вернул `Ok`;
- local/file и SSH сохраняют transport-семантику upstream и требуют доступный
  `git-upload-pack`/`ssh`; отсутствие helper диагностируется без утечки URL и с
  полным rollback;
- `multiple_crate_versions` разрешён точечно: Curl/Rustls и существующие
  cross-platform dependencies требуют несовместимые Windows-only линии
  `windows-sys`, которые нельзя унифицировать из приложения.

Проверки T16: `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test` — успешно
(82 unit, 102 integration). Clone-сценарии покрывают RU/EN, local path,
`file://`, custom remote, существующий destination, невалидный `eska.toml`,
отсутствующий protocol helper и rollback. Ручные RU/EN clone/status выполнены в
отдельном временном каталоге playground.

## T17 — Gix-first миграция реализованных VCS-операций

**Статус:** `IN-PROGRESS`
**Зависит от:** T13, T15, T16

Проверить все production-вызовы system Git и перенести на `gix` каждую операцию,
для которой сохраняются текущие safety guarantees и observable behavior.

Минимальный обязательный scope:

- вынести из T16 общий network/transport слой и выполнять fetch в `eska start`
  через `gix`;
- считать ancestry и обновлять неактивную base ref fast-forward через `gix`;
- не выполнять fetch, если remote из workflow policy отсутствует в repository;
  локальный старт продолжает использовать локальную base;
- оставить system Git только для конкретно перечисленных capability gaps —
  безопасной смены активного worktree/index, hooks, configured editor, signing,
  LFS либо неподдержанного transport/credential сценария;
- capability fallback не должен срабатывать как безусловный повтор после любой
  ошибки `gix`; причина выбора fallback структурирована и тестируется;
- system Git остаётся только в едином infrastructure layer, без parsing human
  output. Итог задачи явно перечисляет все оставшиеся вызовы и причины.

T17 не меняет публичный CLI и не расширяет поведение `start` / `save`.

## T37 — `eska sync` / `eska continue` / `eska abort`

**Статус:** `PLANNED`
**Зависит от:** T13, T15, T17

По policy синхронизировать текущую task branch с base. Если настроенный remote
существует, сначала получить его состояние через gix-first слой T17. Если remote
не настроен, не выполнять fetch и синхронизироваться с локальной base.

`sync`, `continue` и `abort` реализуются одной задачей: конфликт не должен
оставлять пользователя без штатного способа завершить или отменить операцию.
Команды определяют реальное repository state, не хранят его ложную копию и явно
показывают последствия abort. Rebase/merge и их продолжение допустимо выполнять
system Git через изолированный infrastructure layer, пока `gix` не предоставляет
равноценную безопасную orchestration worktree/index.

## T38 — `eska publish`

**Статус:** `PLANNED`  
**Зависит от:** T37

Preflight, требуемая policy синхронизация, push и настройка upstream. Создание
MR/PR и provider integrations оставить следующей задачей.

## T40 — `eska finish`

**Статус:** `PLANNED`  
**Зависит от:** T38, T39

Проверить отсутствие unsaved work и выполнение publish/integration policy; снять
locks, перейти на base, обновить её, удалить локальную task branch и очистить task
state. Remote branch удаляется только по явной policy.
