# Hover текущих типизированных полей

Positional `symbol_info` использует существующий native `SemanticSymbol` для
обычного свойства с известным типом. Карточка получает `kind=field` и тип через
прежний renderer. Наличие реального типа не требует `Definition`: фиктивные
definition, declaration, graph id и координаты перехода не создаются.

Свойство разрешает тот же HIR lookup, который использует текущий receiver.
Неизвестный тип, отсутствующее поле и неполный union не получают выдуманную
карточку. Мягкий scalar hint изменяемой Structure не становится жёстким типом.
Локальный receiver имеет приоритет над одноимённым common module; старый
textual definition route допускается только для незатенённого namespace root.
Вызовы методов сохраняют прежний путь.

## Холодный запрос

Dependency anchor hover теперь принимает и метод, и обычное свойство.
SDK готовит выбранную зависимость через прежний completion/current-input
controller, затем запрашивает карточку для первоначального идентификатора.
Отдельного SDK resolver или чтения всего XML нет.

Холодный canonical descriptor может ещё не разрешать тип результата
`Catalogs.Items.CreateItem()`. Общий native поиск metadata target прослеживает
конкретные reaching assignments aliases и receiver/callee spine до уже
существующего namespace admission. Аргументы, соседние процедуры, перезаписанные
aliases и недоказанная ветвь не разрешают чтение. Циклы ограничены повторным
посещением узла с учётом kind/range/depth. Текущие shadow/environment predicates
сохранены; разрешённый target всё равно проходит обычный source controller.

После загрузки точного owner SDK повторяет native discovery: его тип может
открыть следующий ссылочный owner. Каждый объявленный owner/body допускает
только одну попытку за запрос; циклы не вызывают бесконечную подготовку.
Непричастные descriptors остаются pending. Это адресная цепочка receiver,
а не утверждение полноты всех транзитивных metadata dependencies.

Unsaved metadata меняет тип без изменения BSL; invalid current input отзывает
карточку, repair и close восстанавливают соответствующий текущий источник.
Watcher не заменяет editor-owned ввод. В исходнике сохраняются BOM/CRLF и
Unicode; native byte span карточки отображает именно hovered identifier.

CLI/config/source schemas, версии, native pin и зависимости не менялись.
В экспериментальном Protocol 0.1 standalone hover сообщает
`inputPreparation=member-receiver-dependencies` вместо прежнего
`direct-member-callees`; явная композиция сохраняет `visible-current-composition`.
`field` и его RU/EN заголовки уже существовали для внешних обработок/отчётов;
новые human-readable строки не добавлены.

## Связанная native пара

Предыдущий [строгий probe](native-matching-composition.md) зафиксировал `null`
для поля настоящей совместимой CF/CFE пары, хотя completion уже знал String.
Первоначальные неуспешные результаты сохраняются; новое поведение проверяется
на тех же сохранённых исходниках и на новом frozen release.

Новый framed probe подтвердил `KeyЯ: String` в первом completion и positional
hover, затем повторил completion дважды. Оба roots и selected extension сохранены;
четыре style/API finding составляют полный selected semantic profile.
`semantic_complete=false` остаётся явным. Все 51+28 исходных файлов сохранили
bytes/mode/mtime; процесс завершился штатно. Один RSS sample дал 60 911 616 bytes:
это наблюдение короткого процесса, а не верхняя граница расхода памяти.

Установленный VSIX передал тот же тип через production completion и hover
providers. Диапазон hover совпал с идентификатором; все четыре Problems
совпали с native pull по code, message, severity и range, включая повторяющийся
код. Два собственных процесса завершились. Отдельно повторена прежняя CF/CFE
query-consumer матрица: четыре случая, 33 завершённых процесса. Отказ semantic
context старого несовместимого native CFE сохранился; успешная новая пара
проверена отдельно. Оба набора сохранили исходные fixtures.

## Проверки

Все 27 Rust gates выполнены на одном наборе 1684 Rust/TOML/FTL/lock inputs:
fmt, check, strict Clippy, tests и Rustdoc корня и трёх вспомогательных manifests.
SDK прошёл 262 default и 580 all-features tests; native kernel — 4439, adapter —
123; корень — 1511/1632. В kernel 25 ранее отключённых tests, в корне — 24;
они не считаются пройденными.
Compile и все 376 Node tests прошли без skipped. Все 55 production JavaScript
files совпадают с установленным VSIX; новые host helpers в него не входят.

Предыдущие root отказы сохранены отдельно. Воспроизведена гонка sandbox:
обычный tool call кратковременно создаёт пустой родительский `.git` в playground,
поэтому discovery и импорт получают ошибку repository. Финальный полный root
набор прошёл вне sandbox без параллельных sandbox filesystem calls. Причина
отдельного первого `Shelf(Locked)` не доказана; его isolated replay прошёл.
Production VCS не менялся. Первое сравнение Problems ошибочно зависело от порядка
JSON keys; исправлен только test normalizer с сохранением полных findings.

[Машинные результаты](analyzer-field-hover-results.json).

## Границы MVP

Типизированная карточка не добавляет metadata navigation/editing и не доказывает
общую полноту семантики. Runtime запрос, физический ввод, guarded Save, полный
анализ SDBL/consumers, общий memory budget и оставшаяся приёмка EF-16 ещё нужны.
