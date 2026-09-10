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

**Статус:** `PLANNED`
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
Для установки через `cargo install`, будущий package manager или ручное
копирование она ничего не перезаписывает и показывает способ обновления через
исходный канал. Проверка не выполняется в фоне и не замедляет остальные команды.

`axoupdater` подключается как library внутренней команды; отдельный
`eska-update` не становится пользовательским интерфейсом. Пользователям старых
версий потребуется один раз повторно запустить installer релиза с T57; после
этого следующие releases устанавливаются через `eska update`.

**Готово, когда:** Linux, macOS и Windows обновляются с предыдущего тестового
release на текущий, актуальная версия остаётся без изменений, а отсутствующий,
повреждённый или не относящийся к executable receipt не приводит к записи.
Проверены offline/fake-release сценарии, RU/EN, JSON, `NO_COLOR`, коды выхода и
поведение при недоступной сети или прерванной установке.

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

## T41 — VS Code extension

**Статус:** `PLANNED`  
**Зависит от:** стабильных T12–T18, T21, T27, T37–T39 и JSON protocol

Тонкий frontend: status, start/sync/publish, locking, diagnostics, command palette,
status bar. Не реализовывать VCS повторно в TypeScript; все операции идут через
стабильный core/CLI protocol.
