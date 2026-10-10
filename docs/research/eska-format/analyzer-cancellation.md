# Отмена языковых запросов через IDE-сервер

`eska-analyzer` принимает стандартный
[`$/cancelRequest`](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/specification.md#cancelRequest)
для `textDocument/diagnostic`, [textDocument/completion](analyzer-completion.md),
[textDocument/definition](analyzer-definitions.md) и
[eska/metadata/preparePropertyEdit](analyzer-property-cancellation.md).
Initialize объявляет `requestCancellation=true`,
`cancellableMethods=[textDocument/diagnostic,textDocument/completion,textDocument/definition,eska/metadata/preparePropertyEdit]`
и `maxQueuedMessages=4` внутри `experimental.eska`. Остальные metadata requests, initialize и document notifications
не объявлены отменяемыми. Прежний IDE protocol не изменён.

## Владение и порядок

Один input thread читает framing/JSON и направляет отмену по текущему ID.
Единственный worker по-прежнему владеет общей сессией YAML/BSL, выполняет
запросы, применяет изменения и пишет ответы последовательно. Input thread
не читает project files и не получает database, дерево или текстовую модель.

Обычные сообщения проходят через ограниченную очередь на четыре элемента.
Дополнительно могут существовать выполняющееся сообщение и сообщение reader,
ожидающее места. Это ограничение количества входящих сообщений, не бюджета RSS.
При заполненной очереди действует backpressure: отмена за ещё не прочитанным
обычным сообщением задержится до освобождения места. Отдельной гарантии времени
реакции при перегруженном входе нет.

ID хранится только на время конкретного отменяемого request. Числовые и строковые
ID различаются; Unicode сохраняется. Отмена неизвестного или завершённого ID
игнорируется, не влияет на будущий запрос с таким же ID. Повторный текущий
ID любой из четырёх операций отклоняется как `request_id_in_use`. Перед началом записи ответа guard удаляет ID.

`RequestCancellation` общий для checker и server: он связывает прежний сигнал
ограниченного чтения и `QueryCancellation` текущего kernel handle. Это замена
неопубликованного `batch::BatchCancellation` в Rust API `eska-analysis`.
Compatibility alias не добавлен; CLI/config и JSON checker не изменились.

## Ответ и текущий ввод

Отмена notification не получает отдельного ответа. Исходный request получает
ошибку `-32800`, `data.kind=analysis_cancelled` и текущий snapshot в `details`.
Поздний успешный результат отбрасывается до начала wire write, если отмена
успела прийти. После начала записи ответ может уже завершиться успешно.

Готовый редакторский ввод сохраняет bytes, hash, revision и source quota.
Отмена не является document close и не заменяет черновик сохранённым файлом.
Следующий запрос использует новый control и может проверить тот же текст.

Прерванное lazy-чтение saved BSL оставляет явный unavailable/cancelled input
и новую ревизию по контракту общего host. Следующий pull повторяет только такое
transient saved-чтение. Missing, binary и остальные ошибки не перечитываются
автоматически. Cancelled overlay остаётся текущим unavailable input; fallback
на диск запрещён. Stale snapshot по-прежнему отклоняется до чтения.

Это [кооперативная отмена ядра](query-cancellation.md): код без checkpoint
не прерывается немедленно. Normal shutdown/exit останавливает reader без
ожидания EOF stdin. При фатальной ошибке процесса blocking stdin thread
не удерживает CLI от завершения.

## Проверки

Замеры первой отмены диагностики зафиксированы на commit `80e0e79`.
Последующие проверки приведены в [completion](analyzer-completion.md) и
[отмене property plan](analyzer-property-cancellation.md).

29 unit/file, 22 analyzer process и 9 checker process tests прошли. Проверены
signal handoff без задержек, ID lifetime/reuse, invalid cancellation,
порядок сообщений, остановка reader, cancelled saved retry и запрет overlay
fallback. Process test отменяет запрос над большим черновиком в RU/EN,
проверяет сохранение текущей ревизии/квоты и восстановление последующих запросов.
fmt/check/strict Clippy/test/Rustdoc и release builds прошли для всех tool features.
Root: 887 passed / 14 прежних ignored; minimal 161/3, project-cli 177/3,
build-cli 196/3; обязательные gates прошли.

C1/C2/C3 проверены в обеих локалях. В текущий editor input добавлено около 1 MiB
синтетического корректного BSL; байты corpus не менялись. Отмена отправлялась
после роста RSS child process минимум на 4 MiB и до появления ответа. Это
наблюдение работающего запроса, не точное измерение отдельных parser checkpoints.
Ошибка `-32800` пришла за 0,111–0,118 с. Повторный pull того же большого текста
дал нулевую syntax-диагностику с прежним hash/revision, close восстановил saved.
Эти времена не являются гарантией для иных текстов или окружений.

| Corpus | Peak RSS EN/RU, KiB |
|---|---:|
| C1 | 165252 / 165808 |
| C2 | 297236 / 297364 |
| C3 | 320736 / 321172 |

`PATH` пустой; temporary settings собственных C2/C3 удалены. В каждом corpus
хешированы четыре файла: manifest, configuration, выбранный YAML и BSL.
Они сохранились; остальные файлы не хешировались. Normal exit проверен с открытым
stdin. Machine responses обеих локалей совпали за исключением opaque identities.
Release server — 46729992 bytes; graph 206 package names / 214 resolved versions,
новых dependencies нет.

[Машинный отчёт](analyzer-cancellation-results.json) содержит gates и замеры на `80e0e79`.
Платформа 1С не запускалась: проверялось управление языковыми запросами.
[Server deadline](analyzer-deadline.md) добавлен следующим компонентом;
его проверки и текущий размер сервера описаны отдельно. Полный heap/RSS budget, completion, семантика, watcher,
multi-project host и edit plans остаются в работе. EF-18 — `IN_PROGRESS`.
