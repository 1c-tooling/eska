# Текущий `.sdbl` в общей сессии анализа

Объявленные plain queries CF/CFE/EPF/ERF подключены к той же `AnalysisSession`,
что YAML/BSL. Пути и owner/resource/UUID берутся из проверенного manifest.
При открытии запросы остаются pending без чтения текстов или native ресурсов.
`QueryTextInput` хранит текущие bytes/hash/origin; clock и общий резерв принадлежат
host. Нового `FileId`, BSL-обёртки, XML materialization или второго индекса нет.

Saved/overlay/close/release используют один host revision. Stale revision/version
не меняет вход. Принятый oversized/cancelled input очищает прежние bytes/hash,
сохраняя editor version; анализ не возвращается к сохранённому тексту. Close
читает текущий диск, в том числе его отсутствие. Saved watcher освобождает
готовый saved input без чтения, пропускает открытый overlay и включает `.sdbl`
в существующие source-relative subscriptions. Import/control retirement
сохраняет прежний контракт нового процесса и caller-owned replay.

## Синтаксис и протокол

Проверяется только `QueryParseError` через native `ide::validate_query_text`.
Diagnostic policy учитывается до вызова: `only_enabled` не включает другие rules.
Нативные UTF-8 ranges проверяются на границы текущего текста; transport выдаёт
UTF-16, checker сохраняет byte offsets и добавляет UTF-16 range.
Parse prose RU/EN использует structured native errors.

IDE явно передаёт `initializationOptions.eska.querySyntax=true`. Ответ содержит
`experimental.eska.querySyntax` с negotiated/enabled, languageId `sdbl`,
identifier `eska-query-syntax`, scope `syntax_only`, rules `[QueryParseError]`
и false для metadata/consumer completeness. Без negotiation didOpen/diagnostic
declared query дают `query_syntax_not_negotiated` до принятия input.
Согласованный didOpen требует `languageId="sdbl"`.

`textDocument/diagnostic` использует прежний snapshot/request control и shared
deadline. Ответ связывает source hash, origin, input revision, host snapshot
и config hash; metadata/consumer/semantic completeness остаются false.
Current query state передаётся в `eska/inputChanged`. `metadata/read` и
`metadata/locate` остаются owner-only. Completion/definition не получают
фиктивный BSL-контекст. Отмена проверяется до/после native parser и перед
публикацией; прерывание внутри parser не обещается.

## Checker и бюджет

Checker использует тот же SDK API, читает и освобождает каждый saved query.
Schema 1 дополнена `query_files`, `query_coverage` и `resources.query_text_bytes`.
Query coverage разделяет checked/unavailable/unchecked и syntax completeness;
metadata/consumer completeness всегда false. Прежняя coverage BSL не становится
coverage запросов. Semantic batch с объявленными queries возвращает incomplete
(exit 2), даже при отключённой query rule. Непринятый semantic scope CFE/EPF/ERF
по-прежнему отказывает раньше, по существующему контракту.

Query bytes используют общую source quota YAML/resources/BSL, включая заменяемый
резерв своего прежнего input. Это не heap/RSS budget: retained results и native
parser allocations в source meter не входят. Wire limit остаётся 4 MiB.

## Проверки и границы

Семь direct input/native groups, пять host groups, две batch↔SDK parity groups,
семь actual framed groups и две checker groups покрывают all4, RU/EN, UTF-16,
policy, unsaved/stale/close, deleted file, shared quota, watcher, deadline/cancel,
ID reuse и retirement/replay. Независимое ревью не нашло blockers.
SDK fmt/check/strict all-target/all-feature Clippy прошли; полный default набор —
183 tests, all-features — 445. Повторный detached helper target удалён после
подключения тех же семи tests к библиотеке.

Это синтаксическое подключение в EF-18. Метаданные query, parameters/types,
ресурсы и настройки СКД, DataPath consumers и инвалидация их смысла остаются
EF-27. Current context read-view не выдаёт их за проверенные.
Полная EF-18/EF-27, установленный клиент с `.sdbl` sync и совместная платформенная
приёмка MVP ещё не завершены.
