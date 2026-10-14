# Применение импорта и восстановление выбранного проекта

Это третья часть EF-25: библиотечное применение
[трёхстороннего плана](import-ancestry.md) в существующий проект/member.
Первый профиль сохраняет принятую обратную карту. Пользовательская команда
ESKA reimport и изменение карты были следующими частями. Теперь
[standalone native CLI](native-reimport-cli.md) передаёт preview/apply в тот же
core; обновление IDE ещё не подключено. EF-25 остаётся `IN_PROGRESS`.

## Выбор и граница доверия

`prepare_project_source_import(project, scope, approved, ancestor, incoming)`
сверяет выбранный проект с discovery, включая имя member и унаследованные
настройки workspace. CF/CFE/EPF/ERF определяются по принятому native корню.
Нужен отдельный каталог исходников, не `source = "."`. Source, accepted storage
и служебный каталог не могут пересекаться или проходить через symlink.

`source_plan()` возвращает прежний preview с exact file entries, конфликтами
и исходным pin поддержки. Изменённые правила поставщика требуют EF-15.
Входящий снимок не меняет accepted base, freeze или CI approval.
Если сохранённого состояния ещё нет, caller обязан явно подтвердить общий
синхронизированный предок; рабочие правки не назначаются предком автоматически.
Если состояние уже есть, его ancestor должен совпасть с подтверждённым входом.

`reference()` возвращает SHA-256 неизменяемого журнала. Caller сохраняет этот
`ProjectSourceImportRef` независимо до применения; recovery не выбирает сумму
из прочитанного marker. Это третий тип ссылки, отдельно от `AcceptedRef`
и `ImportAncestorRef`. Все refs и ошибки имеют machine-facing значения.
[Повтор без записи](import-repeat.md) добавляет `resulting_ancestor_reference()`:
при равных data и source прежний confirmed pin может сохраниться, даже если
новый raw incoming snapshot отличается service CDF bytes.

## Публикация

`publish()` и `publish_cancellable()` сохраняют в
`.eska/source-import/<journal-sha256>/` неизменяемый `receipt.json`, полные
снимки `ancestor`/`incoming` и проверенный `candidate`. Receipt закрепляет
старый/новый source revision, approved pin и storage revision, portable пути,
fingerprints settings проекта/workspace и прежнюю state pointer.

Только после проверки staging появляется `.eska-project-source-import.json`.
`EskaSource::open` и `prepare` отказывают с `source_import_incomplete`, в том
числе для handle, открытого до появления guard. Standalone project tool
локализует отказ в RU/EN, JSON code одинаковый. Общий SourceReader не становится
механизмом блокировки IDE: координация watcher/overlays остаётся в EF-25/EF-18.

Текущий source перемещается в `previous-source`, затем `candidate` становится
новым source. Старый каталог остаётся в журнале; автоматического удаления
истории нет. Accepted storage, manifest, attributes, Git и build artifacts
не перезаписываются. После проверки новой композиции обновляется
`.eska/source-import/state.json`, и guard удаляется последним.

State указывает на raw incoming snapshot, а не на результат с локальными
правками. `project_import_ancestor(project, scope, expected)` открывает его
только с independently confirmed `ImportAncestorRef`. Повторный план с тем
же incoming и неизменившимся результатом возвращает `Unchanged` без записи
журнала, настроек или исходников.

## Восстановление

`recover_project_source_import(root, expected, action, cancelled)` работает
без discovery и первоначального incoming staging. Caller явно выбирает
`SourceImportRecovery::Complete` или `RollBack`. Журнал и сохранённые снимки
проверяются по independently retained digest/pins; live source, backup,
candidate, approved storage и settings обязаны совпасть с планом.

При Complete продолжаются незаконченные moves и публикуется incoming pointer.
При RollBack установленный вариант возвращается в candidate, прежний source
возвращается из backup и восстанавливается прежняя pointer, включая отсутствие
первого состояния. Повтор уже выполненного действия — no-op.
Изменённые/лишние файлы, чужая pointer, symlink, неверный receipt/pin и смена
наследуемых settings блокируют оба действия до записи; чужие правки сохраняются.

Отмена проверяется между этапами, filesystem entries и 16 KiB read/copy chunks.
До guard удаляется только собственный незавершённый staging; после guard
сохраняются recovery data, а проект остаётся заблокированным. Отдельный blocking
read/parse не прерывается этим callback. Полностью готовый staging без guard
может повторно использовать исходный план; recovery не начинает неотмеченную
замену автоматически. Частичный store после убийства процесса до готового
receipt не считается подтверждённым и не перезаписывается молча.

Это восстановление после прерывания процесса, а не транзакция при потере
питания или при writer, игнорирующем guard. Source и journal должны поддержать
directory rename в одной файловой системе; ошибка оставляет recovery data.
Несохранённые вкладки IDE ещё не координируются. Большие конфигурации,
Windows/macOS и применение к рабочей ИБ этой частью не принимаются.

## Проверки

11 новых core-сценариев проверяют применение/repeat, обе recovery actions
на четырёх границах, удаление исходного staging, сохранение локальных bytes,
базы и build artifact, stale inputs, изменённые/лишние journal entries,
wrong refs/scope, выбор member, наследование, восстановление прежней pointer,
symlink и отмену во время копирования большого модуля. Process test проверяет
inspect/prepare guard в RU/EN и стабильный JSON.

Все 22 Rust gates прошли: root 779 unit/600 integration, all-features также
21 build/50 project process tests; SDK 185/158/19, adapter и минимальные
build/core/CLI checks пройдены. 24 opt-in случая root suite пропущены и не
считаются подтверждёнными здесь. RU/EN содержат одинаковые 1104 locale keys.

Source-only consumer применил два независимых изменения к каждому настоящему
CF/CFE/EPF/ERF: локальный комментарий BSL и incoming metadata Comment. Реальная
отмена после guard оставила журнал; восстановление завершилось после удаления
входного staging. Подтверждены точные settings/accepted storage, сохранение
контрольного artifact и raw incoming без локального BSL. Повтор плана — no-op.
Результаты собраны непосредственно из обновлённого ESKA через `eska-build`
с исходным CI pin и повторно распакованы на 1С 8.3.27.2325. Получены
13/107/8/4 native файлов и 3/25/3/1 BSL modules; BSL и opaque resources совпали
побайтово, XML — по структуре/attrs/непустому тексту при исключении только
service `configVersion`. Для обработки явно задан контрольный базовый CF.
[Обезличенные результаты](source-import-recovery-results.json).

Workspace MCP анализатора недоступен; CLI 0.3.2 проверил четыре изменённых
native modules. В каждом completed/failed_files=0, SyntaxError отсутствует;
есть 2/5/50/2 style/context diagnostics. `bsl-analyzer.toml` не найден,
использованы defaults. Это syntax check отдельных модулей, не полная семантика
или выполнение в клиенте 1С. Большие production конфигурации не проверены.
