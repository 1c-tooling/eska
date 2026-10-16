# Повторный импорт заимствованного объекта CFE

Добавлена отдельная core-регрессия project three-way reimport: новая поставка
CFE добавляет CommonModule с `ObjectBelonging=Adopted` и отдельным
`ExtendedConfigurationObject`. [Результаты](borrowed-native-reimport-results.json).
Production-код, API, schemas, dependencies и версии этой частью не менялись.

## Проверяемая граница

CFE root и исходный Native-модуль создаются до единственного approval и capture
ancestor. Затем working получает независимый BSL с BOM/CRLF, а native incoming —
новый заимствованный модуль. Preview сохраняет исходные inventories и показывает
оба новых файла как additions: ancestor/working отсутствуют, selected равен incoming.

Project publication возвращает Created. Strict prepare под **прежним** pin
проверяет непосредственно `CommonModule/Properties` в MD namespace: один
`Adopted`, точный target UUID и отдельный descriptor UUID. Проверка не может
пройти за счёт Adopted у CFE root. Соседний модуль остаётся Native; исходный
локальный BSL и новый incoming BSL совпадают byte-for-byte в source и XML output.
Supplier `Reason::OwnObject` не переопределяется через native ObjectBelonging.

Из опубликованной raw ancestry восстанавливается исходный incoming BSL, а не
локальный текст. Новый prepare с сохранённым ancestor и тем же incoming
возвращает Unchanged: весь project inventory, original accepted storage,
старый ancestor, native input и настройки остаются прежними. Approval повторно
не захватывается из working/incoming.

Отдельный test прошёл; свежие root fmt/check/strict Clippy/default/all-features
gates прошли. Default — 1543, all-features — 1664 passed; по 24 ранее ignored,
failed нет. Все 1694 Rust inputs неизменны в течение финального прогона.
От предыдущей root/SDK-приёмки отличается только test-файл; SDK gates повторно
не запускались. Первоначальный Clippy отказал на длине теста; выделение небольших
fixture/assertion helpers сохранило проверки без `allow`.

Это проверка представления и сохранности borrowed metadata через project import,
а не применение нового объекта к реальной CF-базе или запуск платформы 1С.
Совместная платформенная и semantic приёмка относится к EF-16/EF-18.

## Сверка EF-25

| Условие DoD | Принятое доказательство |
|---|---|
| CF/CFE/EPF/ERF, ancestor, local changes, exact preview/repeat, wrong type/member, cancellation | [Native CLI](native-reimport-cli.md), [main CLI](shell-source-import.md), [ancestry](import-ancestry.md) |
| Новый owner, deletion/edit conflict, rename, UUID replacement, binary conflict, freeze | [Addition](import-addition.md), [deletion](import-deletion.md), [rename](import-composition.md) и соответствующие core/process cases |
| Заимствованный owner при project reimport | Отдельная регрессия этой части; прежний initial import/export не подменяет её |
| Support authority, source/base/settings/previous build preservation | [Project recovery](source-import-recovery.md), [post-import bindings](post-import-edit-bindings.md), [native project import](native-project-import.md) |
| Interrupted publication и dirty IDE restart/replay/orphans | [Standalone recovery](installed-native-recovery.md), [workspace 20 cases](installed-native-workspace.md), [retirement](installed-native-retirement.md) |
| Четыре native результата собираются и повторно выгружаются | All4 native циклы в [addition](import-addition.md), [deletion](import-deletion.md), [rename](import-composition.md) и [CLI](native-reimport-cli.md) |

EF-25 — `DONE` в этой проверенной матрице. Core проверяет четыре journal-границы
с обеими recovery actions; installed Linux проверяет настоящий BackedUp для
четырёх типов standalone/workspace. Остальные installed фазы/ОС не заявляются;
отдельной обязательной матрицы всех фаз DoD не задаёт. Общий CLI workflow EF-20,
анализатор, комплект и итоговая приёмка MVP продолжаются.
