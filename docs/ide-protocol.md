# IDE protocol 1.0 — принятый контракт T69

Статус: спецификация принята 2026-09-18; реализована команда `eska ide --stdio` (T70).
Протокол работает поверх T66–T68/T75; исходники, manifest и Git не изменяет.
Допустима запись производного кеша T68. MCP, LSP, редактирование XML,
платформа 1С и зависимость от VS Code в этот контракт не входят.

## Транспорт и конверт

Выбран [JSON-RPC 2.0](https://www.jsonrpc.org/specification): запрос/ответ,
ошибки и notifications. Для потока отдельно выбран framing с Content-Length,
как в [base protocol LSP](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/#baseProtocol).
Это заимствование framing, а не реализация LSP. NDJSON не выбран: длина байтов
даёт однозначную границу независимо от переводов строк в JSON.

Кадр: ASCII `Content-Length: <N>\r\n\r\n`, затем ровно N байт JSON UTF-8,
без BOM. N — десятичное положительное число байтов, не символов.
Допустим необязательный `Content-Type: application/vscode-jsonrpc; charset=utf-8`.
Имена заголовков нечувствительны к регистру; неизвестные заголовки игнорируются.
Повтор Content-Length, неверная длина/кодировка и LF вместо CRLF — fatal framing
error. Приём должен выдерживать любое разбиение на chunks и несколько кадров
в одном read. Каждый output frame записывается одним владельцем writer, без
перемешивания; stdout не содержит логов, ANSI, prompts или BOM.

Лимиты версии 1.0: header 8 KiB; входной body 1 MiB; исходящий body 64 MiB;
входной JSON nesting 64; batch до 16 элементов; до 128 pending requests и суммарно
4 MiB принятых, но ещё не выполненных bodies. Превышение header/body или
невосстановимая граница кадра закрывает процесс с exit 2, без попытки искать
следующий заголовок внутри мусора. Корректно обрамлённый неправильный JSON
получает `-32700`, id null; следующий кадр можно обрабатывать.
Превышение входной JSON depth даёт Invalid Request с id null, без исполнения.

Каждый request: `jsonrpc:"2.0"`, `id`, `method`, `params` object.
Для методов без параметров params можно опустить; positional params не
поддерживаются (`-32602`). ID — непустая строка до 64 ASCII-символов или целое
число 0..9007199254740991; null/дробные/прочие IDs дают `-32600`, id null.
Клиент не переиспользует ID в течение подключения, включая batch.
Сервер хранит только pending IDs; повтор уже завершённого ID не проверяется.
Повтор pending ID — нарушение корреляции: fatal exit 2, ранее начатая операция не
обещает отдельного ответа. Числовой 1 и строковый "1" различаются.

Batch обрабатывается по порядку элементов; requests дают один массив ответов,
notifications не включаются. Пустой batch — `-32600` с id null; malformed
элемент — отдельный Invalid Request. Если batch больше 16, requests получают
`resource_limit`, notifications пропускаются; malformed элементы получают Invalid Request.
Ни один метод не выполняется.
Если batch содержит только notifications, ответа нет. Результаты batch
буферизуются с резервом 8 KiB для оставшихся error responses; если очередной response превысит общий исходящий лимит, вместо
него включается `response_too_large`. Это не откатывает уже выполненный refresh:
актуальное поколение приходит отдельным событием. Одиночный слишком большой
response также заменяется ошибкой; частичные DTO не отправляются.

Notification не имеет id и никогда не получает ответ. Неизвестные notifications
игнорируются; неверные параметры известных журналируются в stderr.
Request-only методы без id не выполняются (diagnostic в stderr). Notification-only
методы с id получают Method not found без эффекта. До initialize разрешён также
exit; прочие notifications игнорируются, requests получают invalid_state.
Неизвестные поля object игнорируются для расширения minor-версии, но неизвестные
значения enum/неверные типы известных полей отклоняются. Дубли ключей JSON —
Invalid Request (id null). Сервер не посылает requests клиенту.

## Версия, подключение и время жизни

`initialize` — первый request и единственный до handshake. Params:
`{apiVersion:{major:1,minor:0},client:{name:string,version:string},locale}`.
locale: `ru-RU|en-US`, default `en-US`. Сервер отвечает
`{apiVersion,server:{name:"eska",version},capabilities,limits}`.
Major должен совпасть, minor клиента не выше поддержанного; иначе
`unsupported_version`. Повтор initialize — `invalid_state`.
Поля limits: maxHeaderBytes=8192, maxRequestBytes=1048576,
maxResponseBytes=67108864, maxDepth=64, maxBatchItems=16,
maxPendingRequests=128, maxQueuedBytes=4194304.
Версия IDE API отделена от crate, CLI JSON и формата дискового кеша.
Добавление необязательных полей/capabilities — minor; изменение обязательных
полей/семантики — major. Сериализация Rust/дискового кеша не является wire DTO.

Capabilities 1.0: `designerXml:true`, `readOnly:true`, `diskCache:true`,
`search:true`, `clientFileEvents:true`, `batch:true`, `multiContext:false`,
`supportedProjectTypes:["configuration","extension","processing","report"]`.
Один процесс держит один открытый eska discovery context (standalone или
workspace с несколькими members). Несколько несвязанных manifest contexts
в одном IDE workspace в 1.0 не объединяются; клиент выбирает активный context.
Повтор workspace/open до workspace/close даёт `workspace_already_open`.

`workspace/open` params: `{start:Path,selection,diskCache?}`.
start — абсолютный путь на стороне процесса. selection — `{kind:"current"}`,
`{kind:"all"}` или `{kind:"named",names:[string,...]}`. Это существующие правила
selection: standalone принимает только current; all/named — для workspace;
current из workspace root выбирает всех members; из member — текущий проект.
Пустые/повторные/неизвестные names отклоняются. Нужен `eska.toml`; XML без manifest
не открывается, автоматического init нет. diskCache default true; false вызывает
обычный read-only open. Выбор и открытие атомарны для публикации session:
ошибка любого выбранного проекта не публикует частичную session.
Заполненные до ошибки файлы производного кеша допустимы.

Результат open: `{sessionId,projects:[ProjectInfo]}`. sessionId — непрозрачная
строка, не переиспользуется в рамках процесса. При новом подключении клиент
сбрасывает все прежние IDs, ответы и event sequences независимо от их значений.
ProjectInfo: `{projectId,scope,type,rootPath:Path,sourcePath:Path,root:NodeId,
 generation,eventSequence,requiresRefresh:bool,requiresReopen:bool}`. projectId — непрозрачный token сессии;
scope — `{kind:"standalone"}` или `{kind:"member",name:string}`.
Тип берётся из manifest и проверяется по XML; ProjectInfo ещё не означает
готовность полного поискового индекса. Открытие не запускает индексацию.

`project/info({sessionId})` возвращает тот же `{sessionId,projects}` со свежими
поколениями. `workspace/close({sessionId})` прекращает индексацию, освобождает
данные и отвечает null. Уже принятые запросы выполняются FIFO; последующие
запросы со старой session получают `unknown_session`. Файлы кеша остаются.

`shutdown({})` допустим после initialize, с открытой session или без неё:
закрывает её, отвечает null и переводит процесс в closing. В closing допустим
только `exit` notification; requests получают `invalid_state`.
`exit` после shutdown завершает процесс с 0; без shutdown — с 1.
EOF между кадрами отменяет pending работу, закрывает session и завершается с 0, без новых ответов;
EOF внутри кадра — exit 2. Broken pipe stdout — exit 1, без дальнейших записей.
На stderr идут ограниченные diagnostics без исходного XML/BSL и ANSI.

## Общие DTO

Ниже запись `?` означает необязательное поле; остальные поля обязательны.
Все payload keys и enum literals — английские, не зависят от locale.

- `Path = {value:string,encoding:"utf-8"|"percent"|"utf-16-percent"}`:
  правила T49 из `src/cli/encoding.rs`. UTF-8 сохраняется буквально, включая `%`;
  Unix fallback кодирует **все** байты `%HH`, Windows fallback — все units `%HHHH`.
  Это native path, не URI; кодировка другой ОС отклоняется. Пути sources и
  file events относительны sourcePath; `..`, абсолютные пути и выход через
  symlink отклоняются. Клиент не конструирует URI из percent-строки без decoding.
- `ObjectId` — непрозрачная строка T60 (например `catalog:Контрагенты`), которую
  клиент получает от сервера; не выводится из имени и не содержит project scope.
- `NodeId` — discriminated object: `{kind:"object",objectId}`;
  `{kind:"module",owner:ObjectId,role}`;
  `{kind:"collection",owner:ObjectId,collection}`.
  collection: `{kind:"metadata",metadataKind}` или `{kind:"common"|"modules"|"unsupported"}`.
  metadataKind — `MetadataKind::as_str()` T60; module role — `ModuleRole::as_str()`.
  Никаких индексов массивов или локализованных подписей в identity.
- `Text = {language:string,content:string}`; синонимы — массив Text всех языков XML.
- `Label = {kind:"name",text}` для имени из XML или
  `{kind:"key",key,translations:{"ru-RU":string,"en-US":string}}` для схемы.
  Label передаёт обе локали; initialize.locale выбирает язык UI клиента, не
  меняет DTO. Клиент не переводит сами имена/синонимы конфигурации.
- `Range = {start:number,end:number}` — полуинтервал UTF-8 bytes исходного XML
  с учётом BOM/CRLF, не UTF-16 координаты редактора.
- `Diagnostic = {code,range:Range|null,details:object}`; codes из parser issues
  в snake_case: unsupported_root, unknown_kind, invalid_object,
  duplicate_identity, unsupported_value. details содержит исходные поля issue
  (namespace/name/objectId), либо пустой object; localized message не требуется.
- `Object = {objectId,metadataKind,name,parent:ObjectId|null,uuid:string|null,
  synonyms:Text[]}`. UUID неизвестной ссылки может быть null.
- `Node = {id:NodeId,parent:NodeId|null,label:Label,state,expandedByDefault,
  rootSection,diagnostics:Diagnostic[]}`; state:
  `empty|non_empty|unloaded|error`. Список children получают отдельным запросом.
- `Property = {key:{namespace:string|null,name},qualifiers:[{key,value:string}],
  value,range:Range}`. value: `{kind:"text",text}`, `{kind:"localized",items:Text[]}`,
  `{kind:"record",fields:[{key,qualifiers,value},...]}` или
  `{kind:"unsupported",issue:"mixed_content"|"invalid_localized_text"}`.
  Порядок и повторы fields сохраняются; отсутствующие запрошенные properties — [].

Generation и eventSequence передаются десятичными строками unsigned u64, чтобы
JavaScript не округлял большие значения. Они локальны project/session;
переполнение требует переоткрытия session. Начальные значения — "0".

## Запросы метаданных

Каждый метод этого раздела принимает обязательные
`{sessionId,projectId,generation,...}`. Несовпадение generation перед исполнением
даёт `stale_generation`. Успех обёрнут в
`{sessionId,projectId,generation,eventSequence,...}` с состоянием на момент
ответа. Даже при ошибке refresh текущее generation доступно в error.data.

| Метод | Дополнительные params | Дополнительные поля результата |
| --- | --- | --- |
| `metadata/root` | нет | `node:Node` |
| `metadata/children` | `node:NodeId,hideEmptyRootSections?:bool` (default true) | `nodes:Node[]` в порядке Конфигуратора |
| `metadata/get` | `objectId` | `object:Object` из уже открытой навигации |
| `metadata/properties` | `objectId` | `properties:Property[]` |
| `metadata/source` | `node:NodeId` | `sources:[{path:Path,role,inline:[{metadataKind,name}]}]` |
| `metadata/search` | `text:string,limit?:1..500,synonymLanguage?:string` | `progress:Progress,hits:Hit[],truncated:bool` |
| `metadata/reveal` | `objectId` найденного элемента | `ancestry:NodeId[]` |
| `metadata/refresh` | `node:NodeId` | `affected:ObjectId[]` и новое generation |
| `metadata/index` | `action:"start"|"cancel"|"resume"|"status"` | `progress:Progress` |
| `metadata/indexErrors` | `offset?:number,limit?:1..500` | `errors:[{objectId,code,details}],nextOffset:number|null` |

Source role: `{kind:"descriptor"}`, `{kind:"payload"}` или `{kind:"module",role}`.
Виртуальные группы sources не имеют: existing core MissingSource отображается
как domain error, а не путь к выдуманному XML. Бинарные модули без BSL скрыты.
get/properties не выполняют произвольный поиск неизвестного ID; для результата
поиска сначала reveal. Сервер проверяет reveal по текущему индексу и строит
ancestry сам; клиент не передаёт доверенный массив предков.

Progress: `{state,indexedObjects,pendingDescriptors,failedDescriptors}`;
state: `not_started|building|cancelled|ready|incomplete`. Hit:
`{objectId,node:NodeId,metadataKind,name,synonyms:Text[],ancestry:NodeId[],rank}`.
Обёртка search задаёт scope/generation для всех hits. Ранги:
`exact_name|exact_synonym|prefix_name|prefix_synonym|substring_name|substring_synonym`.
Правила matching/сортировки — [T75](metadata-search.md), default limit 50.
Пустой запрос возвращает пустую выдачу; поиск никогда не запускает индекс сам.
При неполном индексе UI показывает progress, не утверждает отсутствие объекта.
indexErrors offset default 0, limit 50, порядок ObjectId; offset больше длины
возвращает пустой массив и null. Пока индекс строится, список ошибок меняется:
клиент запрашивает его заново после окончания работы, а не объединяет страницы
из разных progress событий. Для malformed notifications без определимого
session/project сервер пишет stderr; клиент восстанавливается по отсутствию
подтверждения ожидаемого changed или таймауту. code/details используют mapping ошибок ниже. Для IndexFailure::Unsupported
code=unsupported_metadata, details={diagnostics:Diagnostic[]}.

Отмена/запуск индекса не меняют generation исходников. metadata/index start
перестраивает поисковые записи, переиспользуя кеш, но не обнаруживает пропущенные
file events: для этого нужен refresh. Смена root-фильтра — только представление;
не изменяет IDs, generation и вложенные группы. Группа модулей идёт первой,
expandedByDefault следует T65; клиенты не заменяют пользовательское раскрытие
при каждом новом ответе этим первоначальным значением.

## Планировщик и отмена

Reader принимает кадры независимо от worker, сразу отмечает `$/cancelRequest`
notification `{id}` для активного/queued request. Worker исполняет один request
за раз, FIFO; между запросами — не более одного descriptor индексации.
Несколько готовых проектов индексируются round-robin. Ограниченная очередь
даёт `resource_limit` новым requests; reader продолжает принимать отмену.
Если очередь заполнена batch, лимит проверяется до запуска любого его request.
Notifications индекса коалесцируются; критические invalidation events не теряются.
При backpressure writer приостанавливает worker, не накапливая неограниченный
буфер (лимит writer queue — 128 MiB). Клиент обязан постоянно читать stdout. Закрытие канала останавливает
reader/worker/writer; worker проверяет сигнал между ограниченными шагами.

Не начавшийся отменённый request получает error `cancelled` с исходным id.
Активный read request проверяет отмену до публикации ответа: вычисления/кеш могут
сохраниться, но ответ заменяется cancelled. Обработка XML не прерывается внутри
parser. Для lifecycle, refresh и index-control точка фиксации состояния является
границей отмены: отмена до начала прекращает операцию, после фиксации возвращается
обычный result/error, состояние не откатывается. Отмена уже отвеченного или
неизвестного ID игнорируется. Каждый request имеет ровно один response.

Индексация — отдельная фоновая работа: index start отвечает сразу после постановки
очереди. Отмена этого уже отвеченного request её не останавливает; используется
index cancel. Последний один descriptor может закончиться до применения cancel.
metadata/search использует готовые записи и не ждёт готовности полного индекса.
Интерактивные запросы имеют приоритет над индексом; непрерывная нагрузка клиента
может задержать его завершение. T68 измерил шаг из восьми descriptors до 104 мс;
T70 использует один и повторно проверяет задержку через реальный процесс.

## Изменения файлов и события

Watcher принадлежит клиенту, backend сам файловую систему не сканирует в фоне.
`workspace/didChangeFiles` notification params:
`{sessionId,projectId,sequence:string,paths:Path[],manifestChanged?:bool}`.
sequence начинается с "1", возрастает на 1 для каждого проекта. Один пакет —
не более 4096 путей; пустой paths без manifestChanged — invalid params.
Клиент группирует изменения до отправки; rename передаёт старый и новый пути,
изменение объявлений — также XML владельца. Сохранять изменения в source через
этот метод нельзя: это сообщение о уже записанных файлах.

Worker применяет события в порядке входа относительно обычных запросов.
Успешный пакет инвалидирует core T67 и даёт серверное notification
`metadata/changed {sessionId,projectId,generation,eventSequence,affected:ObjectId[]|null,
 requiresRefresh:bool,requiresReopen:bool}`. eventSequence — отдельный серверный
счётчик всех metadata/changed этого проекта; refresh тоже создаёт такое событие.
Событие отправляется до ответа следующего запроса и до response самого refresh.
Если список affected не помещается в кадр, null означает инвалидизацию всех
ветвей на клиенте, без установки requiresRefresh на сервере только из-за размера.
Клиент всегда отбрасывает ответы с меньшим generation или eventSequence и хранит максимум
полученного eventSequence, даже если response batch пришёл позже события.

Неверный/пропущенный/повторный sequence, некорректный path, переполнение очереди
file events или ошибка применения пакета не игнорируются молча: проект
помечается requiresRefresh и посылает metadata/changed. Сервер увеличивает
свой eventSequence даже если core generation не поменялось. До полного refresh
методы метаданных, кроме root refresh и index status, возвращают resync_required.
Простой root() не считается восстановлением. Клиент также делает root refresh
при пропуске серверного eventSequence или потере watcher.

Для восстановления `metadata/refresh` корня принимает дополнительно
`resetFileSequence?:bool` (default false). При true после полного успешного
refresh ожидается sequence "1", прежние pending file events проекта удаляются;
клиент останавливает watcher, дожидается ответа, затем возобновляет его с "1" и
делает второй root refresh для окна между остановкой и запуском watcher.
При false ожидаемый sequence после успешного refresh становится последним
увиденным валидным числом + 1. Перед root refresh из состояния resync_required
проверка старого generation пропускается; ответ сообщает новое.

manifestChanged или изменение состава workspace требует close/open всей
session: changed с requiresReopen true; metadata methods до переоткрытия дают
reopen_required. Ошибка корневого XML сохраняет повышенное core generation и
requiresRefresh; успешный refresh исправленного XML снимает флаг.
После каждого обновления индекс становится частичным; reader/worker не
публикует старые hits как текущие. В новой session прежние generations не годятся.

`metadata/indexProgress {sessionId,projectId,generation,progress}` отправляется
при смене состояния и не чаще раза в 100 мс при изменении счётчиков. Состояние
после cancel и окончательное состояние отправляются обязательно.
Ошибка отдельного XML отражается в progress/indexErrors; не завершает процесс.
После закрытия session её notifications больше не отправляются.

## Ошибки

JSON-RPC стандартные codes: -32700 Parse error; -32600 Invalid Request;
-32601 Method not found; -32602 Invalid params; -32603 Internal error.
Domain code = -32000, message = "Request failed", data:
`{kind,sessionId?:string,projectId?:string,generation?:string,details:object}`.
message и kind стабильны и не локализуются. details — структурированные данные,
не Display/Debug Rust errors; paths используют Path DTO. Неподдержанные write
методы, включая metadata/updateProperty, дают Method not found.

| kind | Источник / детали |
| --- | --- |
| unsupported_version | requested, supported версии |
| invalid_state / workspace_already_open | state |
| project_open_failed | reason: manifest_missing, manifest_invalid, selection_invalid, source_invalid, root_invalid; path? |
| unknown_session / unknown_project | запрошенный token |
| unknown_node / unknown_object | node или objectId |
| source_missing / source_invalid / xml_invalid | path?, objectId?, reason; без содержимого файла |
| source_changed | path; клиент делает refresh |
| stale_generation | expected (сервер), actual (запрос), обе строки |
| resync_required / reopen_required | reason |
| generation_exhausted | требуется close/open |
| cancelled | пустой details |
| resource_limit | resource: queue, batch, file_events; limit |
| response_too_large | limitBytes |

Core BrokenAncestry/неожиданный TreeError/паника — Internal error без внутренних
адресов/stack trace в wire; details диагностики идут в stderr. SourceError
не теряет различие IO/missing/containment, reason: io, outside_source,
not_file, ambiguous, invalid_identity, too_large, unsupported. LoadError::Parse
даёт xml_invalid с reason too_large, too_deep, malformed или invalid_metadata.
Recoverable parser diagnostics сохраняются в Node и indexErrors, не заменяются
фатальной ошибкой. UnknownProject/Object/Node и SourceChanged напрямую
соответствуют одноимённым core категориям.

## Fixtures и приёмка T70

[Fixtures сообщений](ide-protocol/fixtures.json) содержат handshake/open,
каждый metadata method, отмену, file events, batch и закрытие.
[Кадры](ide-protocol/frames.wire) — UTF-8 тела с рассчитанной длиной байтов;
это иллюстративные примеры контракта, **не записанные ответы конкретного стенда**.
Их значения IDs/путей иллюстративны, воспроизводимые данные core находятся
в tests/fixtures/designer. Для reveal используется точечный core lookup записи индекса по ObjectId;
существующий reveal_search_hit не заменяется поиском по имени с лимитом выдачи.
T70 обязан проверять DTO конверсию отдельно от
serde формата дискового кеша, включая все node/value variants и T49 paths.

Acceptance T70: разбиение кадра по каждому байту, склейка кадров, кириллица,
wrong length/UTF-8/depth/headers, parse error и восстановление следующего кадра;
batch (включая notifications-only), IDs, лимиты очереди/ответов, отмена во время
индексации, открытие четырёх типов, несколько members, все методы, malformed
XML и пропущенные file events; close/reopen/EOF/shutdown/broken pipe.
RU/EN должны давать одинаковые machine DTO, включая Label с обеими локалями.
XML/BSL, manifest и Git сравниваются побайтно до/после; только при diskCache=true
разрешены изменения `.eska/cache/metadata`. Для diskCache=false — никаких записей.
Бюджеты [T68](metadata-performance.md) повторяются через stdio; до этого они
не являются доказательством производительности protocol-процесса.


## Реализация T70

CLI adapter расположен в `src/cli/ide`: отдельные reader и writer, последовательный
worker, ограниченная очередь и атомарные флаги отмены. Новых зависимостей нет.
Reader проверяет framing/envelope и отмечает отмену до исполнения обычной очереди.
Writer — единственный владелец stdout. На idle worker выполняет один descriptor
индекса, чередуя проекты; запросы имеют приоритет. Ожидание свободного writer
проверяет остановку, поэтому EOF не оставляет worker заблокированным.

Диагностика stderr ограничена 32 короткими машинными кодами на процесс.
Переполнение очереди файловых уведомлений консервативно требует refresh всех
открытых проектов; критическое уведомление об этом не теряется. Полный refresh
с `resetFileSequence:true` удаляет pending events этого проекта, включая остаток batch.
При завершении процесса ОС закрывает также поток, ожидающий чтения stdin;
workspace и его кеш в памяти освобождаются worker. Дисковый кеш сохраняется.

Отдельный клиент в `tests/project/metadata_workspace/ide.rs` проверяет реальный
executable через pipes, без повторного использования server framing или DTO.
Unit-тесты отдельно проверяют границы сообщений, переполнение очереди, DTO видов
метаданных/модулей/значений и обратимость путей. Реальные process-проверки выполнены
на Linux; Windows/macOS runtime-приёмка остаётся обязательной при поставке клиента.
Замеры процесса и команда воспроизведения — [T70 performance](ide-performance.md).
