# Лимит времени диагностики и автодополнения

`eska-analyzer --timeout-seconds 300` задаёт кооперативный лимит одного
`textDocument/diagnostic`, [textDocument/completion](analyzer-completion.md) или
[eska/metadata/preparePropertyEdit](analyzer-property-cancellation.md).
Значение по умолчанию — 300 секунд; initialize
объявляет его как `experimental.eska.requestTimeoutSeconds`. Ноль останавливает
проверку до lazy-чтения BSL. Metadata requests и document notifications остаются
доступными. Настройка относится к процессу, не к `eska.toml`.

## Общий механизм

Server и checker используют один внутренний `Deadline` и `RequestCancellation`.
Таймер не владеет database или исходным текстом: он останавливает ограниченное
чтение и зарегистрированный handle ядра. Первый сигнал фиксирует причину;
поздняя клиентская отмена не превращает timeout в `analysis_cancelled`, а timeout
не заменяет уже принятую отмену пользователем. После завершения запроса timer
пробуждается и освобождается сразу, без ожидания исходного лимита.

Таймер начинается после проверки params, snapshot, settings, профиля и пути.
Он охватывает lazy-чтение, kernel query, преобразование диапазонов и построение
report. Поздний результат отбрасывается перед возвратом ответа. Initialize,
ожидание в очереди, wire serialization и blocked stdout не входят в этот лимит.
Код без checkpoint не прерывается немедленно; это не гарантия времени ответа.

## Ответы и ввод

[LSP pull diagnostics](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/language/pullDiagnostics.md)
допускает server cancellation с управлением повторным запросом.
Сервер возвращает исходный ID, `-32802`, `data.kind=analysis_deadline_exceeded`,
`data.retriggerRequest=false` и текущий snapshot в `data.details`. Так клиент
не запускает бесконечный повтор того же слишком дорогого запроса.
Клиентская отмена сохраняет `-32800` и `analysis_cancelled`.
Completion использует `-32000` с analysis_deadline_exceeded без retriggerRequest:
специальный `-32802` остаётся ответом diagnostic pull.

Timeout не закрывает документ и не меняет готовые editor bytes/hash/revision
или source quota. Следующий запрос получает новый control. Прерванное saved
lazy-чтение остаётся unavailable/cancelled и повторяется следующим pull по
[контракту общего host](analyzer-cancellation.md); editor fallback запрещён.
Ошибки preflight сохраняют собственные коды. Непредставимая длительность даёт
`analysis_timeout_out_of_range`, невозможность создать timer —
`analysis_timer_unavailable`; обе ошибки имеют код `-32000`.

Checker сохраняет прежнюю схему `deadline_exceeded`/`cancelled` и exit 2 для
неполного отчёта. Новых dependencies, config keys или compatibility aliases нет.

## Проверки

Замеры этого раздела и машинного отчёта зафиксированы на commit `9b45392`.
Позднее подключено [completion с тем же timer](analyzer-completion.md);
новые проверки и размеры бинарника приведены в его отчёте.

32 unit/file, 24 analyzer process и 9 checker process tests прошли вместе с
fmt/check/strict Clippy/test/Rustdoc для всех tool features и обоими release builds.
Root: 887 passed / 14 прежних ignored; minimal 161/3, project-cli 177/3,
build-cli 196/3. Полный обязательный набор прошёл.

Детерминированный gate подтверждает работающий tracked query до сигнала таймера;
его parser checkpoint прерывается, входы сохраняются и следующий query проходит.
Отдельно проверены первая причина остановки, ноль, overflow, освобождение timer,
оба языка, preflight, неизменность saved/editor и независимое чтение метаданных.

На C1/C2/C3 в RU/EN проверены два режима. Нулевой лимит не загрузил BSL, не изменил
ревизию/квоту и не помешал следующему metadata read. Для активного запроса открыт
синтетический корректный BSL размером 3600044 bytes; лимит 1 секунда дал `-32802`
за 1,043–1,081 с. Это наблюдение конкретного стенда, не SLA. Snapshot и source
quota сохранились; close восстановил saved hash, следующий маленький запрос
в том же процессе дал нулевую syntax-диагностику. Поздняя отмена завершённого ID
не помешала его повторному использованию. Normal exit проверен с открытым stdin.

| Corpus | Peak RSS активного запроса EN/RU, KiB |
|---|---:|
| C1 | 948216 / 947872 |
| C2 | 1079288 / 1079692 |
| C3 | 1102816 / 1103164 |

Эти замеры ещё раз показывают, что source-byte quota не ограничивает AST/memos
или RSS. `PATH` пустой; временные настройки собственных C2/C3 удалены. В каждом
corpus сохранились четыре проверенных файла: manifest, configuration, выбранные
YAML и BSL. Остальные файлы не хешировались. Machine responses локалей совпали
за исключением opaque identities.

Release server — 46734912 bytes; runtime graph 206 package names / 214 resolved
versions. [Машинный отчёт](analyzer-deadline-results.json) сохраняет gates и замеры.
Платформа 1С не запускалась: проверено управление запросом, не исполнение BSL.
Полный heap/RSS budget, семантика, watcher, multi-project host и
edit plans остаются в работе. EF-18 — `IN_PROGRESS`.
