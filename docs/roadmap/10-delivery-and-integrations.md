# Анализ, delivery и integrations

## T24 — `affected` analysis

**Статус:** `PLANNED`  
**Зависит от:** T20, T21, T23

Определять potentially affected objects/tests для changed objects; интегрировать с
`check --affected` и будущим `test --affected`. Human и JSON output обязательны.

## T25 — Versioning проекта 1С

**Статус:** `DONE`
**Зависит от:** T03, T20

`version`, `version bump patch|minor|major`; позднее `auto` по Conventional Commits,
semantic changes и policy. Никогда не смешивать project version с версией бинарника
`eska`.

T25 реализована для всех поддерживаемых типов Designer XML. `version` читает
четырёхкомпонентное значение корневого `Properties/Version`; `bump` сопоставляет
major/minor/patch с revision/subrevision/version, сбрасывает младшие компоненты к
начальным значениям и сохраняет ширину числовых частей.

Запись не использует XML serializer: в исходном массиве байтов заменяется только
текст единственного прямого тега `Properties/Version`. BOM, CRLF, пробелы,
атрибуты, вложенные одноимённые теги и остальной XML сохраняются побайтно.
Невалидный, неоднозначный или превышающий 64 МиБ descriptor отклоняется до записи.
Human output локализован; JSON-схема версии 1 не зависит от locale. Git, build
artifact и версия crate не изменяются. `auto` выделен в T56.

## T56 — Автоматическая подготовка версии релиза 1С

**Статус:** `PLANNED`
**Зависит от:** T18, T22, T25, T54

Добавить `eska version bump auto` для Release PR/MR по модели release-plz.
Команда анализирует локальные commits от последнего release tag до выбранного
revision и определяет максимальное изменение версии по Conventional Commits:
breaking change — `major`, `feat` — `minor`, `fix`/`perf` — `patch`;
`docs`/`test`/`ci`/`chore` сами по себе релиз не создают. Результат применяет
существующую четырёхкомпонентную семантику T25 и меняет только текст прямого
`Properties/Version`.

Для одного проекта использовать теги `v<revision.subrevision.version.build>`,
для workspace — `<project.name>/v<version>`. Тег и текущий XML должны описывать
одну опубликованную версию; расхождение, отсутствие первого baseline или
неоднозначные теги требуют явного `--since` либо исправления истории. Workspace
анализирует только commits, затрагивающие выбранный member. Общие изменения
workspace не повышают все версии молча: нужна явная release group или выбор
проектов.

`--dry-run --format human|json` показывает baseline/tag, диапазон commits,
причину bump, текущую и следующую версии, проект и descriptor без записи.
Обычный запуск повторяет расчёт и оставляет точный XML diff для release branch;
он не создаёт commit, tag, PR/MR, artifact и не обращается к сети. Повторный
запуск на том же baseline не должен увеличивать уже подготовленную версию ещё
раз.

GitHub Actions и GitLab CI остаются тонкими adapters: запускают dry-run и bump,
фиксируют diff в release branch и создают Release PR/MR средствами provider.
Одна и та же core-команда работает локально и в обоих CI. Токены, provider API,
публикация и генерация CI-конфигурации в core не добавляются.

**Готово, когда:** одинаковая commit history даёт одинаковый план независимо от
locale и CI provider; проверены major/minor/patch/no-release, mixed commits,
первый релиз, неверный/неоднозначный tag, повторный запуск и detached revision.
Workspace покрывает независимые версии, member path scope и общие изменения.
XML diff остаётся byte-minimal; RU/EN, стабильный JSON и примеры Release PR для
GitHub и Release MR для GitLab задокументированы.

## T57 — Обновление установленного `eska`

**Статус:** `IN-PROGRESS`
**Зависит от:** действующей публикации бинарных releases через cargo-dist

Добавить явную команду `eska update`, которая проверяет последний стабильный
release бинарника и обновляет установку, созданную shell/PowerShell installer.
Использовать install receipt и release installer cargo-dist/axoupdater; не
собирать URL artifacts вручную и не заменять бинарник неизвестного происхождения.

`eska update --check` только сообщает установленную и доступную версии. Обычный
`eska update` при наличии новой версии запускает обновление, а на актуальной
версии завершается успешно без изменений. Human output локализован; JSON имеет
стабильную схему и различает `up-to-date`, `update-available`, `updated` и
`unsupported-installation`.

Команда обновляет только executable, которому соответствует install receipt.
Для установки через Cargo проверяется `cargo install --list --root <root>`
и принадлежность запущенного executable этой установке. Обновление запускается
через Cargo с сохранением каталога и параметров установки. При ошибке Cargo
перехода на установщик нет. Неизвестные установки не перезаписываются. Проверка не выполняется в фоне и не замедляет остальные команды.

`axoupdater` подключается как library внутренней команды; отдельный
`eska-update` не становится пользовательским интерфейсом. Пользователям старых
версий потребуется один раз повторно запустить Cargo или installer релиза с T57; после
этого следующие releases устанавливаются через `eska update`.

**Готово, когда:** Linux, macOS и Windows обновляются с предыдущего тестового
release на текущий, актуальная версия остаётся без изменений, а отсутствующий,
повреждённый или не относящийся к executable receipt не приводит к записи.
Проверены offline/fake-release сценарии, RU/EN, JSON, `NO_COLOR`, коды выхода и
поведение при недоступной сети или прерванной установке.

### Реализовано в T57

`eska update [--check] [--target-version X.Y.Z] [--format human|json]`
работает без проекта. Cargo проверяется по фактическому install root и
`.crates2.json`; сохраняются root, target, profile и features. Проверка Cargo
обращается к crates.io, проверка installer — к GitHub Releases. Автоматического
перехода между каналами и понижения версии нет. Обновление блокируется от
параллельного запуска и проверяет версию установленного файла после завершения.

`axoupdater` использует небольшой отдельный Tokio runtime только внутри update;
IDE runtime и остальные CLI-команды остаются синхронными. В handshake API 1.1
добавлен `selfUpdate`; API 1.0 остаётся допустимым для клиентов.
Публичный контракт: [cli-update.md](../cli-update.md).

На Linux выполнены изолированные тесты fake release/installer, ошибки установки,
проверки RU/EN и JSON. Полный набор fmt/check/Clippy/test прошёл: 176 unit и 345 integration,
4 ранее отключённых теста пропущены. Human `update --check` проверен в RU/EN.
Дополнительно 2026-09-19 на Linux x86-64 проверены официальные установщики:
чистая установка 0.11.0 и переход 0.10.1 → 0.11.0 через bootstrap-код Explorer.
Оба сценария использовали отдельные временные home в соседнем playground;
пользовательская установка не менялась. Подтверждены PATH, flat install receipt,
сохранение каталога и IDE handshake новой CLI.

Это проверка installer/bootstrap, а не полного self-update: опубликованная
0.11.0 ещё не содержит T57. Текущая сборка, помещённая только в тестовую установку
с настоящим receipt, дошла до сетевой проверки и вернула JSON `error/network`
с кодом выхода 1. GitHub API в этом сеансе отвечал HTTP 403; Assets скачивались.
На этом промежуточном этапе полная смена версии командой `eska update`,
Cargo-обновление и Windows/macOS ещё не были проверены. Результаты после
публикации 0.11.1 приведены ниже; T57 остаётся `IN-PROGRESS` до проверки всех ОС.

### Самообновление до опубликованной 0.11.1 (Linux)

После публикации 0.11.1 проверена реальная замена бинарника через `eska update`.
Исходная точка — сохранённая development-сборка 0.11.0 с T57, помещённая в
одноразовую официальную установку 0.11.0. Это не переход между двумя
опубликованными версиями с T57: опубликованная 0.11.0 команды ещё не имела.

`update --check` вернул `update-available` без изменения SHA-256; затем
`update --target-version 0.11.1 --format json` скачал настоящий installer и
бинарник релиза и вернул `updated`, method `installer`. Проверены `--version`,
receipt 0.11.1 и прежний install prefix. Уже опубликованная 0.11.1 вернула
`up-to-date` для RU/EN и обычного повторного update без изменения бинарника.

Публичный GitHub API продолжал отвечать 403. Только метаданные релиза получались
через локальный посредник с `gh api`, без изменения JSON; installer и бинарник
скачивались из GitHub Assets. Следовательно, прямой неавторизованный API этим
прогоном не подтверждён. Установка располагалась в уникальном временном home
в соседнем playground и удалена после теста. Пользовательская CLI не менялась.

### Исправление проверки Cargo

В реальной установке через Cargo обнаружено: `cargo install` работает, но
`https://crates.io/api/v1/crates/eska` возвращает 403 и блокирует `update --check`.
Проверка переведена на `https://index.crates.io/es/ka/eska` — официальный индекс
Cargo. Разбираются записи версий JSON Lines; исключаются yanked, prerelease,
чужие имена пакета и неподдерживаемые схемы. Лимит ответа 4 MiB сохранён.
Bootstrap Explorer проверяет публикацию в том же индексе. Это изменение источника
версий, без смены способа установки, новых зависимостей или изменений JSON CLI.
Формат: [Cargo Registry Index](https://doc.rust-lang.org/cargo/reference/registry-index.html).

После исправления выполнена реальная Cargo-установка 0.11.0 в отдельный prefix
с `--debug`. В одноразовой копии исходников собрана проверяемая реализация как
development 0.11.0; версия в репозитории не менялась. `update --check` успешно
нашёл 0.11.1, `update --target-version 0.11.1` выполнил настоящий `cargo install`
и вернул `updated`. Подтверждены бинарник 0.11.1 и сохранение root, profile `dev`,
target, features/all_features/no_default_features. Затем отдельно проверена
актуальность исправленной сборки 0.11.1 в этой установке: `up-to-date`.
Опубликованная 0.11.1 ещё содержит прежний запрос к Web API; исправлению нужен
следующий релиз. Все временные source/target/install каталоги удалены.

Финальные Rust-проверки: fmt/check/Clippy и 521 тест прошли, 4 прежних теста
пропущены. Windows/macOS и прямой неавторизованный GitHub API остаются
ограничениями приёмки; T57 пока `IN-PROGRESS`.

## T32 — Release pipeline

**Статус:** `PLANNED`  
**Зависит от:** T27, T28, T56

Policy-driven pipeline: determine/validate/update version, changelog, commit, tag,
build, artifacts. Полный `--dry-run` обязателен до write/destructive действий.
T32 использует рассчитанную T56 версию и добавляет полную подготовку changelog,
tag, сборку из чистого tagged commit и публикацию artifact/manifest.

## T33 — CI/CD integration

**Статус:** `PLANNED`  
**Зависит от:** T23, T26–T28, T32

Одинаковые `fmt --check`, `check` и `build` локально и в CI; `test` включается,
когда отдельная implementation task по принятой T23 добавит backend. Будущий
`ci init` генерирует тонкие adapters для GitLab CI/GitHub Actions без business
logic; `eska` не становится CI server.

## T41 — eska: 1C Explorer

**Статус:** `PLANNED`  
**Зависит от:** T70, T75, T76–T81 для первой поставки

Сводная задача Explorer, декомпозированная в [T76–T81](14-vscode-extension.md).
Репозиторий — `eska-vscode-explorer`, package name — `eska-explorer`.
Первый результат — устанавливаемое расширение с деревом Конфигуратора,
первой группой модулей, поиском, фильтром пустых групп, иконками и переходом
к XML/BSL. `.bin` без BSL скрыты. Обязательный `eska.toml` определяет тип проекта
и схему дерева; отсутствующий manifest не создаётся автоматически.

Клиент отображает schema/узлы от core, не сканирует Designer XML и не запускает
CLI заново для каждого раскрытия. Серверная подготовка и поиск вынесены в
[T60–T70, T75](13-metadata-and-ide.md). T41 закрывается после приёмки T81,
дополнительного самостоятельного implementation шага для T41 нет.

Сборка/выгрузка, поставка, запуск приложения, Git и locking не входят в Explorer:
это отдельное будущее расширение команд eska, независимо устанавливаемое.
Собственные команды навигации T76–T79 входят в первую версию Explorer.
BSL language server, grammars, форматтер и отладчик предоставляет выбранное
пользователем расширение; Explorer их не регистрирует и не требует bsl-analyzer.
Совместная работа с bsl-analyzer проверяется в T81.
Редактирование существующих свойств возможно после T73 через логический
`metadata/updateProperty`, структурные операции — только после T74. Клиент не
редактирует XML самостоятельно; визуальный редактор форм сюда не входит.

Большой стенд — `big_configuration_temp` в соседнем `eska-playground`.
Он используется только для чтения и замеров; сборка этого проекта запрещена
пользователем и не входит ни в backend-проверки, ни в приёмку расширения.
