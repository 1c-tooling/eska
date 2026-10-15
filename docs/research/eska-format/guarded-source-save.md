# Защищённое сохранение исходников

Обычный Save в экспериментальном Explorer проходит через writable
`eska-edit` FileSystemProvider и `eska/source/save`. Core проверяет весь кандидат
и публикует точные UTF-8 bytes одним replacement. Provider не пишет native файл.
Readonly `eska-protected` сохраняет прежнее назначение; ручное сохранение
отдельно открытого `file:` не считается защищённым Save.

Наблюдатели обоих provider и SDK принимают только native `file:` events:
отражённое виртуальное событие не возвращается в тот же watcher. Открытие
исходника сохраняет проверки проекта, сеанса и declaration. Если собственный
`didOpen` обновляет дерево, отбрасывается прежний selection range, а показывается
актуальный `TextDocument`, включая замену документа при назначении языка.

Ниже описана реализация текущего среза. Финальные Rust gates прошли; результаты
Node и приёмка установленного VSIX фиксируются отдельно и здесь пока не заявлены.

## Разрешения и файлы

Save доступен только для объявленного открытого текста выбранного проекта при
независимо заданном startup pin принятой базы. Semantic profile и
`workspaceEdit.documentChanges` не выдают право записи и не требуются для этого
канала. Foreign/base/hidden inputs, binary и произвольные XML не принимаются.
Создание, удаление и переименование файлов этим provider не выполняются.
Канал общий для CF/CFE/EPF/ERF; это не заменяет установленную матрицу четырёх типов.

BSL использует прежний module planner, отдельный SDBL — query planner.
Синтаксическая ошибка в черновике не запрещает хранить текст модуля или запроса;
query сохраняет прежние ограничения размера и XML characters.
Сохранение запроса дополнительно сверяет исходный текст, захваченный planner,
с проверенными bytes запроса: конкурентное изменение не становится новой базой
для этого Save.

Для YAML новый `prepare_yaml_edit` принимает изменения существующих typed
properties. Каждый изменённый accepted UUID/property проходит прежний CST
preview. Последовательность разрешённых splices должна воспроизвести **все**
bytes кандидата. Mixed-inline файл не требует Resource всего документа:
разрешённое свойство сохраняется при frozen родителе или соседе. Любой
запрещённый delta отклоняет весь Save, без промежуточных записей и без молчаливого
восстановления полей.

Точное no-op YAML имеет отдельный private вариант: проверяет pin, binding,
объявленный path и saved bytes, сохраняет timestamp, не заимствует permission
произвольного объекта. Изменённый план повторяет все property claims перед
одной публикацией. Property `Comment` и комментарии самого YAML различаются:
первое проходит обычный typed planner, произвольная trivia пока не принимается.

## Protocol 0.1

`experimental.eska.sourceSave` сообщает method `eska/source/save`, scope
`selected-open-document`, sourceKinds `yaml/bsl/sdbl`, предел request 4 MiB
и cancellation `before-commit`. Без независимого pin capability отсутствует.

Запрос содержит `snapshot`, `source`, `document_version`, `draft_sha256`,
`expected_disk_sha256` и полный `text`. Source задаётся относительно выбранного
корня. Snapshot, открытый Overlay, wire version, длина и hash должны совпасть
с уже принятым journal input до чтения принятой базы.

`draft_sha256` относится к логическому тексту редактора; `expected_disk_sha256`
и результирующий `disk_sha256` — ко всем bytes файла. Поддерживаемый UTF-8 BOM
BSL/SDBL остаётся на диске, хотя VS Code убирает его из `getText()`. CRLF и Unicode
не нормализуются. YAML BOM и некорректный UTF-8 явно отклоняются.

Ответ содержит прежний `snapshot`, `source`, `document_version`,
`draft_sha256`, `disk_sha256` и `outcome`: `published` или `already_current`.
Save не закрывает Overlay, не меняет его version/revision и не создаёт второй
writer. Последующий `didSave` также сохраняет editor authority.
Save, Save All и autosave используют один канал. Записи одного native path
сериализованы; отказ сохраняет dirty buffer, а подтверждение отправленной версии
не заменяет более новый черновик. Отдельного journal или прямой записи из клиента
нет.

Если actual disk соответствует expected hash, core проверяет и публикует
кандидата. Если actual уже равен результату, fresh guards подтверждают no-op
`already_current`. Это наблюдение текущих bytes, а не утверждение, что прежний
запрос этого backend выполнил запись. Третий hash даёт conflict.

## Публикация и конкуренция

Bound editor plans, project import и recovery используют общий OS lock
`.eska/source-write.lock`. Его path остаётся стабильным после освобождения
guard: файл не удаляется, чтобы следующие writers не захватили разные inode.
Guard явно снимает lock перед закрытием handle. Root, `.eska` и lock file
проверяются на aliases; lock должен быть обычным файлом. Busy немедленно даёт
`source_write_busy`.
Незавершённый import по-прежнему требует явного recovery.
Свободный core API для source root без project binding сохраняет прежнюю границу
и не получает project lock.

Под lock повторяются принятый pin, permissions, source binding, exact bytes
и настройки выбранного проекта/workspace. Fresh discovery проверяет source,
accepted path и inherited settings. Captured hashes защищают последующие
изменения settings; отдельный reader-to-binding manifest fence не позволяет
подменить карту между проверками. Accepted storage и pin не обновляются.
Изменение resolved project или workspace inheritance после инициализации
отклоняется. Эквивалентный комментарий в settings, уже существующий до свежего
захвата guard, допустим; после захвата любое отличие сохранённых settings bytes
отклоняется до записи.

Отмена проверяется непосредственно перед rename/hard-link. После успешной
записи typed committed outcome не проходит через read-result cancellation
или import fence. Потеря ответа сохраняет неопределённость у клиента:
он удерживает прежнюю пару expected/result hashes. Повтор с новым черновиком
может выбрать только один из этих двух наблюдённых disk hashes для нового
conditional request; посторонние bytes автоматически не принимаются.

Только успешный `published` typed-property Save выбранного root YAML создаёт
private fingerprint пути и полных сохранённых bytes. `already_current` не создаёт
новое доказательство публикации. При точном совпадении безопасно прочитанных bytes
последующий root watcher использует обычный Overlay watch, включая объединённые
delete/create. Это сохраняет dirty siblings при Save All с root YAML.
`ReadPermissions` принимает новый hash только соответствующего выбранного root;
состав, independent pin и правила разрешений остаются прежними.

Отсутствующий root, alias или другие bytes требуют restart. Foreign root,
`source.json`, project/workspace/analysis settings и import не получают этого
исключения. Сверка fingerprint не выдаёт новую authority и не заменяет свежие
guards следующего Save.

Atomic replacement не является OS compare-and-swap против внешнего writer,
который не участвует в общем lock. Отдельная power-loss transaction не добавлена.

## Проверки

В итоговом наборе прошли все 28 gates: 7 корневых и 21 для SDK, kernel
и adapter. Все завершились с exit 0 на последнем production-коде.
Пройдены fmt/check, strict all-target/all-feature Clippy, warnings-denied Rustdoc,
сборка debug binaries и release binaries SDK. Default и all-feature профили SDK
проверены отдельно.

| Набор тестов | Passed | Failed | Ignored |
|---|---:|---:|---:|
| Root default | 1542 | 0 | 24 |
| Root all-features | 1663 | 0 | 24 |
| SDK default | 264 | 0 | 0 |
| SDK all-features | 599 | 0 | 0 |
| Kernel workspace all-features | 4439 | 0 | 25 |
| Adapter all-features | 123 | 0 | 0 |

[Машинный результат](guarded-source-save-results.json) содержит команды,
counts, hashes бинарников и VSIX. Каждый Rust phase использовал frozen набор
из 1694 inputs; before/after внутри phase совпали. Между SDK и итоговым root
исправлена только assertion нового root-only integration-теста: существующий
YAML decoder допускает некоторые нестандартные escapes, но публикация обязана
их отклонять. Все production/config/locale/lock inputs фаз совпадают; этот
единственный test-only delta указан в JSON. Текущий root SHA-256:
`2a87e64e033dd93b340f657181df3eac1ac00a4ecd755b352e71ab45f424e6d8`.

TypeScript check/compile и проверка VSIX прошли. Все 58 production JS совпали
с упакованными bytes; test helper в VSIX отсутствует. Полный Node gate —
404 passed, 0 failed/cancelled/skipped. Before/after 175 client inputs совпали.
Регрессии включают literal Unicode, escaping, собственный didOpen, смену языка
и отсутствие feedback от виртуальных filesystem events.

Установленный production VSIX со свежим release analyzer выполнил 28 настоящих
публикаций: по семь для CF/CFE/EPF/ERF. Ctrl+S BSL/root YAML, Save All с четырьмя
одновременно dirty файлами и autosave сохранили точные bytes, BOM/CRLF, один
сеанс и остальные черновики. Хеши остальных исходников и accepted history не
изменились; новый пустой project lock проверен отдельно. Матрица query consumers
прошла четыре проекта, matching CF/CFE — completion/hover/Problems. Все 40
дочерних analyzer processes завершились; Save/matching дополнительно проверены
по kernel start time через pidfd, без принудительного завершения.

Ранние ошибки и их отчёты сохранены отдельно. В этом Save-срезе платформа 1С
не запускалась; scripted native-editor transactions не выданы за физический
ввод человеком или полный native build/runtime цикл.

## Оставшееся до полной приёмки

Whole-YAML Save пока ограничен существующими typed properties. Изменения структуры
и содержимого форм, произвольная trivia, добавление/удаление свойств и участников,
исправление уже некорректного saved YAML требуют следующих validators. Новый канал
не расширяет покрытие самого property planner и не объявляет готовыми все IDE edits.

Полные семантика BSL/SDBL, memory budget, поставка EF-21 и сквозная native/client
приёмка EF-16 остаются в работе. Этот срез не исполняет runtime запрос в 1С.
