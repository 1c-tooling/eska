# Первое соединение IDE с общей сессией

`eska-analyzer` — отдельный экспериментальный бинарник в `tools/analysis-core`.
Feature `analyzer-cli` включает только его process boundary; `check-cli` собирает
пакетный checker независимо, а библиотека без CLI features остаётся без транспорта.
Оба потребителя используют один `AnalysisSession`. Проект и member выбираются
тем же selector, что в project/build/check; `rootUri` клиента не переопределяет
выбор процесса. Сейчас один процесс обслуживает один выбранный проект.

## Согласование и жизненный цикл

Используется stdio с [framing и жизненным циклом LSP 3.17](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/specification.md).
ESKA methods согласуются отдельно от существующего IDE protocol приложения 1.x.
CLI help/version локализованы; нормальный stdout содержит только framed JSON-RPC.
Строковые ID сохраняют Unicode, числовые ID — signed 32-bit integers LSP.
Batch JSON-RPC, null ID, повторные JSON keys и глубина более 64 отклоняются.

Первый request:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "capabilities": {},
    "initializationOptions": {"eska": {"protocolVersion": "0.1"}}
  }
}
```

Несовместимая версия отклоняется до discovery или чтения проекта. Успешный ответ
содержит `capabilities.experimental.eska`: версию, source format `eska`, область
`selected-project`, metadata capabilities, ограничения wire и первый `snapshot`.
Затем клиент отправляет стандартное уведомление `initialized`.
До него custom requests получают `-32002`; повторный initialize отклоняется.
`shutdown` отвечает `null` и освобождает сессию; уведомление `exit` после него
даёт exit 0. Exit без shutdown, потерянный pipe или неверная граница frame дают 1.
Ошибки аргументов до запуска транспорта дают 3; help/version — 0.

`documentSync` уже true: [full-document синхронизация YAML/BSL](analyzer-documents.md)
подключена к той же сессии. `languageQueries=true` включает пока только
[`textDocument/diagnostic` с профилем syntax](analyzer-diagnostics.md).
`requestCancellation=true` включает [отмену syntax-запросов](analyzer-cancellation.md);
`editPlans=false`. Существующие расширения не переводятся
на новый процесс автоматически; это ещё не полный языковой сервер.

## Текущий custom API

Каждый request содержит `snapshot` с `session_id` и `revision`. Первый — свежий
UUID каждого успешного открытия процесса; второй — непрозрачная десятичная
строка. Клиент копирует обе величины из ответа. Равный счётчик другой сессии
не принимается; строка сохраняет ревизии за пределом точности JSON numbers JS.
При reconnect все старые ответы и токены отбрасываются.

| Метод | Params кроме snapshot | Результат |
|---|---|---|
| `eska/metadata/owners` | `offset` по умолчанию 0, `limit` от 1 до 200 | Проверенный состав, текущие состояния, total, offset, resources; YAML не читается |
| `eska/metadata/read` | `source` — объявленный путь YAML | Текущий YAML-снимок; pending загружается с диска один раз |
| `eska/metadata/refresh` | `source` | Явное повторное saved-чтение с новой ревизией, включая неудачный результат |

`read` возвращает текущий черновик открытого YAML, не заменяя его диском.
`refresh` открытого документа отклоняется с `overlay_active` (`-32000`).

Например, для синтетического владельца:

```json
{
  "jsonrpc": "2.0",
  "id": "properties",
  "method": "eska/metadata/read",
  "params": {
    "snapshot": {"session_id": "<copied UUID>", "revision": "0"},
    "source": "CommonModules/Tools/object.yaml"
  }
}
```

Каждый успешный ответ возвращает новый текущий `snapshot`. Lazy read или refresh
могут повысить ревизию; параллельно отправленный старый токен отклоняется.
`stale_revision` имеет код `-32801` и текущий snapshot; `unknown_session` — `-32000`.
Неверные params, page limit, неизвестный/небезопасный source дают `-32602`.
Проверка snapshot и объявленного пути выполняется перед disk read.
Неизвестный метод даёт `-32601`. Custom params не допускают лишних полей.

Владелец показывает origin, source hash, input readiness и состояние адаптера
раздельно. Неполнота, unsupported/invalid projection и quota refusal остаются
явными. Неудачный refresh заменяет прежнюю модель; результат не возвращает
старые свойства или успешно пустое дерево. После исправления нужен новый refresh.
`read` сам не повторяет failed-чтение.

Metadata DTO содержит логические ID, kind/name/UUID/parent/children, synonyms,
typed properties, references и parse issues по каждому native descriptor.
Properties используют общий `MetadataProperty`/`MetadataValue`: это значения
модели, не AST или структуры Salsa. `range` измеряется UTF-8 bytes в текущем
YAML и привязан к source hash/snapshot. Inline-команды видны у своего владельца.
Двоичные payload и неподдержанные модели BSL этим API не раскрываются как код.

## Ограничения ресурсов

Один worker обрабатывает запросы последовательно. Отдельный reader принимает
отмену и держит ограниченную очередь обычных сообщений; подробности —
в [контракте отмены](analyzer-cancellation.md).
Framing этого сервера ограничивает header 8 KiB, вход 4 MiB, ответ 64 MiB; malformed
framing завершает процесс без поиска следующей предполагаемой границы.
Oversized response возвращает coded error `response_too_large` с текущим
snapshot; wire buffer ограничен до записи header. Оценка DTO до сериализации,
composition и ядро не входят в этот wire cap.

`--max-document-bytes` сохраняет hard bound 64 MiB. Общая текущая
[квота исходных байтов](source-budget.md) по умолчанию 256 MiB и используется
той же сессией. Это не общий heap/RSS budget. Страницы/свойства не читают BSL;
BSL-текст поступает через document sync или lazy-чтение для диагностики.
Отмена активной syntax-диагностики подключена; полная семантика ещё не готова.
Состав неизменяем до restart; structural changes manifest/configuration требуют
перезапуска. Watcher и multi-project host пока не подключены.

## Реальные проверки

Ниже сохранён первый read-only замер на `f199144`; текущая синхронизация,
размер бинарника и проверки описаны в [следующем компоненте](analyzer-documents.md).

Проверены C1/C2/C3: initialize, все страницы владельцев, чтение одного common
module owner, shutdown/exit. До read все owner inputs оставались pending, source
quota равнялась нулю. После read загружено 4104/4070/4070 source bytes; BSL texts — 0.
Ответы EN/RU одинаковы за исключением свежего session UUID. Пиковый RSS и время
относятся к одному ограниченному сценарию, не полной семантической проверке.


| Corpus | Owners | Initialize EN/RU, s | Peak RSS EN/RU, KiB |
|---|---:|---:|---:|
| C1 | 2645 | 0.068 / 0.074 | 38180 / 38456 |
| C2 | 16286 | 0.635 / 0.643 | 171928 / 172088 |
| C3 | 18752 | 0.805 / 0.733 | 195444 / 195300 |

206 имён packages (214 разрешённых версий); Git/HTTP/LSP runtime/отладчик
не входят в граф. Размер release-бинарника — 44219096 bytes.

Source map, configuration YAML и прочитанный owner byte-equal до/после запуска.
Все остальные исходники в этом замере не хешировались. `PATH` пустой;
временные настройки только собственных imported C2/C3 удалены.

24 unit/file tests, 8 process tests analyzer и 9 checker прошли; также пройдены
отдельные минимальный, checker и analyzer profiles, fmt/check/strict Clippy/Rustdoc.
Приложение прошло полный набор: 886 passed / 14 прежних ignored; minimal — 161/3,
project-cli — 176/3, build-cli — 195/3. Framing и strict JSON вынесены в общую
CLI-инфраструктуру; прежний IDE handshake, его ошибки и ограничения сохранены.
Новых dependencies нет; UUID API уже предоставляет закреплённое ядро metadata.

Сборка из корня репозитория:

```bash
cargo build --manifest-path tools/analysis-core/Cargo.toml --features analyzer-cli --release --locked --bin eska-analyzer
```

В выбранном проекте запускается полученный бинарник:

```bash
eska-analyzer --project-dir .
```

Его ещё нет в установленном комплекте, путь нужно задать самостоятельно.
[Машинный отчёт](analyzer-metadata-results.json) содержит итоговые замеры и gates.
EF-18 остаётся `IN_PROGRESS`: остальные language operations,
полное покрытие метаданных, memory budget и защищённые edit plans ещё не готовы.
