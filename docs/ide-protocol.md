# IDE protocol 1.x — базовый контракт и расширения

Статус: спецификация принята 2026-09-18; реализована команда `eska ide --stdio` (T70).
Базовый режим поверх T66–T68/T75 не изменяет исходники, manifest и Git.
Допустима запись производного кеша T68. API 1.6 добавляет явное включение записи
существующих свойств; контракт приведён ниже. MCP, LSP, запуск платформы 1С
и зависимость от VS Code в протокол не входят.

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

Базовые capabilities 1.0: `designerXml:true`, `readOnly:true`, `diskCache:true`,
`search:true`, `clientFileEvents:true`, `batch:true`, `multiContext:false`,
`supportedProjectTypes:["configuration","extension","processing","report"]`.
Сервер API 1.1 также сообщает `selfUpdate:true`: CLI поддерживает отдельную
команду `eska update` и её JSON-контракт. Это не разрешение на обновление через
read-only IDE RPC. Клиент 1.0 продолжает работать; отсутствие поля означает, что
вызов `update` по этому признаку недоступен.

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
  rootSection,metadataKind:string|null,diagnostics:Diagnostic[]}`; state:
  `empty|non_empty|unloaded|error`. Список children получают отдельным запросом.
  Для конечных объектов без дочерних коллекций backend сразу возвращает `empty`,
  не читая их XML. Если тип поддерживает модули, проверяется наличие BSL-файлов;
  при их наличии узел остаётся `unloaded`. Это состояние пересчитывается при
  изменениях файлов. Отсутствие загруженного списка children само по себе
  не доказывает пустоту узла.
  `metadataKind` — machine kind объекта, включая inline-элементы; для групп и
  модулей null (их тип/роль уже есть в NodeId). Поле добавлено в API 1.0
  обратно совместимо: старые клиенты игнорируют неизвестные поля; новые клиенты
  при отсутствии поля у старого backend используют универсальную иконку объекта.
- `Property = {key:{namespace:string|null,name},qualifiers:[{key,value:string,caption?}],
  value,range:Range,caption?:{"ru-RU":string,"en-US":string}}`. value: `{kind:"text",text,caption?,scalarType?:"boolean"}`, `{kind:"localized",items:Text[]}`,
  `{kind:"record",fields:[{key,qualifiers,value,caption?},...]}` или
  `{kind:"unsupported",issue:"mixed_content"|"invalid_localized_text"}`.
  Порядок и повторы fields сохраняются; отсутствующие запрошенные properties — [].
  `caption` — дополнительное необязательное поле с обеими локалями; оно не
  зависит от `initialize.locale`. Каталог учитывает namespace и контекст
  владельца, неизвестное свойство не получает caption. Клиент использует
  исходное `key.name` при отсутствии подписи, в том числе со старым backend.
  Для известных перечислений и встроенных типов `value.caption` имеет ту же
  форму; `qualifiers[].caption` применяется к стандартным идентификаторам.
  Исходные `text`, `value`, machine-readable ключи и варианты `kind` не меняются.
  `value.scalarType = "boolean"` — необязательная подсказка для известных булевых
  полей платформы. Исходный `text` сохраняется; допустимы `true`, `false`, `1`, `0`
  с окружающими пробелами. Клиент без подсказки показывает текст: одно лишь
  совпадение строки с `true` не определяет её тип. Поле не зависит от locale.
  Подписи учитывают тип поля; пользовательские строки не переводятся. Источники, покрытие и
  версии 8.3.27/8.5.1 описаны в [каталоге платформы](platform-catalog.md).

С API 1.5 capability `propertyPresentation:true` добавляет необязательное
`presentation` к Property и вложенным fields. Старые формы `value`, исходные
значения, квалификаторы и byte ranges сохраняются. Неизвестный `presentation.kind`
клиент отображает через исходный `value`.

- `{kind:"empty",caption:Caption}` — `xsi:nil=true|1`, явно типизированные пустые
  строки, Undefined/Null либо пустой известный селектор объекта.
- `{kind:"reference",caption:Caption,category:Caption,metadataKind,status,target?}` —
  ссылка на метаданные. `status`: `resolved`, `missing`, `unavailable`;
  `target:ObjectId` присутствует только при `resolved`, в текущем projectId.
- `{kind:"types",items:[PresentedItem]}` — распознанное описание типов целиком.
  `PresentedItem` содержит `caption:Caption`, необязательный `detail:Caption`
  для ограничений, а для типа объекта — также поля reference без обязательного
  `kind`. `Caption = {"ru-RU":string,"en-US":string}`.

Ссылки распознаются по разрешённому QName `xsi:type=MDObjectRef|DesignTimeRef`
либо в проверенных контекстах Designer XML: селекторы форм, макетов, стилей,
языков, признаков учета, хранилищ, функциональных опций, картинки и пути полей. Пользовательский текст не распознаётся по одному лишь
совпадению с `Catalog.Name`. Разрешаются только объявленные объекты выбранного
проекта: загружаются их предки и descriptor цели через обычный ограниченный кеш,
без запуска поискового индекса. Синоним выбирается для каждой локали, затем
используется другой непустой синоним или точное имя. Отсутствующая/нечитаемая цель
не прерывает чтение остальных свойств. Неизвестные типы и ограничения сохраняют
исходную структуру. `metadata/reveal` принимает также эти уже известные дереву
identity и возвращает актуальную ancestry без индексации.

`DesignTimeRef` различает пустую ссылку, значение перечисления и предопределённый
элемент. Последний ищется по уникальному имени только в `Predefined.xml`
владельца; неоднозначное имя не создаёт переход. Ссылка на стандартный реквизит
открывает свойства владельца; селектор процедуры — свойства общего модуля.
Подпись объясняет направление перехода, имя поля/процедуры остаётся видимым.
Вложенные ссылки могут содержать `detail:Caption` с подписями предков.

`TypeSet` разрешается по URI пространства имён аналогично `Type`.
Определяемые типы и характеристики ссылаются на соответствующие метаданные;
семейства «Любой документ», «Любая ссылка» отображаются без фиктивной цели.
XML-префиксы XDTO-типов не используются как подписи, исходный текст сохраняется.

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
| `metadata/properties` | `objectId` | `properties:Property[],picture?:PicturePreview` |
| `metadata/source` | `node:NodeId` | `sources:[{path:Path,role,inline:[{metadataKind,name}]}]` |
| `metadata/search` | `text:string,limit?:1..500,synonymLanguage?:string` | `progress:Progress,hits:Hit[],truncated:bool` |
| `metadata/reveal` | `objectId` найденного или уже известного дереву элемента | `ancestry:NodeId[]` |
| `metadata/refresh` | `node:NodeId` | `affected:ObjectId[]` и новое generation |
| `metadata/index` | `action:"start"|"cancel"|"resume"|"status"` | `progress:Progress` |
| `metadata/indexErrors` | `offset?:number,limit?:1..500` | `errors:[{objectId,code,details}],nextOffset:number|null` |

С API 1.4 capability `picturePreview:true` означает, что `metadata/properties`
добавляет `picture` для `common-picture`. У других типов поле отсутствует.
`PicturePreview` — `{status:"ready",mimeType:string,data:string,fileName:string}`
или `{status:"missing"|"unsupported"|"invalid"|"too_large"|"unavailable"}`.
`data` содержит стандартный Base64 исходных байтов; имя файла служит подписью,
а не путём для открытия. Ошибка превью не отменяет получение свойств.
Поля и статусы не зависят от языка. Старые клиенты могут игнорировать это поле.

Читается только файл из `CommonPictures/<имя>/Ext/Picture.xml` →
`Picture/<xr:Abs>`. Поддерживаются PNG, JPEG, GIF, BMP, ICO, WebP и SVG;
MIME определяется содержимым. В ZIP выбирается вариант без `interfaceVariant`,
затем SVG или вариант с наибольшей объявленной площадью. Некорректный манифест
не заменяется произвольным изображением из архива: возвращается статус ошибки.
Если манифеста нет, используется SVG или файл с наибольшим числовым именем,
затем лексический порядок.
ZIP поддерживает Stored/DEFLATE; распаковки на диск нет. Лимиты: архив 32 MiB,
картинка 8 MiB, каждый XML 64 KiB, 512 записей архива. Ссылки содержат только
имя файла; выход через symlink за source запрещён. Бинарные данные не кешируются:
повторный запрос читает актуальную картинку. Декодирование изображения и его
размеров выполняет клиент; неподдерживаемые браузером варианты дают ошибку превью.

Предопределённые элементы имеют `metadataKind:"predefined-item"`. Их группа —
обычный `collection` с `collection:{kind:"metadata",metadataKind:"predefined-item"}`;
она загружает Predefined.xml при `metadata/children`. Элементы сохраняют
иерархические ObjectId, sources указывают на Predefined.xml с ролью descriptor.
Свойства и диапазоны `Name` используют namespace `http://v8.1c.ru/8.3/xcf/predef`.
Description доступен в поиске как `synonyms` с языком `und` (XML не задаёт язык).
Форма существующих запросов и ответов не менялась.

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
expandedByDefault для группы модулей равен false (T65); клиенты не заменяют
пользовательское раскрытие при каждом новом ответе этим первоначальным значением.

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
методы дают Method not found. Для metadata/updateProperty и metadata/undoProperty
без явного opt-in возвращается property_read_only.

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
индекса, чередуя проекты; запросы имеют приоритет. Без запросов и фоновой работы
worker спит до поступления входа, завершения записи или остановки процесса.
Ожидание свободного writer
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

## Аддитивное расширение 1.2: правила поддержки

Capability `supportPolicy` и постраничный `metadata/support` описаны в
[правилах поддержки Designer XML](support-policy.md). Старые методы и значение
`readOnly` не меняются. Без capability клиент не должен вызывать новый метод
или объявлять защиту исходников действующей.

`metadata/changed.supportUnchanged` — дополнительный boolean: `true` подтверждает,
что это событие не изменило кеш поддержки и соответствия файлов (например,
сохранено содержимое существующего BSL). Клиент может перенести эти данные на новые
токены поколения/события. При пропуске событий, requiresRefresh/requiresReopen или
отсутствующем поле данные необходимо проверить заново. Побайтно неизменившийся
`ParentConfigurations.bin` сам по себе не вызывает metadata/changed.

## Аддитивное расширение 1.3: адресная проверка поддержки

Capability `supportFiles` разрешает `metadata/supportFiles`. Запрос принимает
обычный контекст `sessionId`, `projectId`, `generation` и `paths`: от 1 до 128
source-relative путей в стандартном обратимом DTO `Path`. Пустые пути,
абсолютные пути и переходы к родителю отклоняются. Ответ содержит контекст,
`eventSequence`, `objects`, `files`, `diagnostics`, `freshness: "current"`.
Форматы объектов и файлов совпадают с `metadata/support`; `files` сохраняет
порядок и число запрошенных путей. `nextOffset` и `suppliers` не выдаются.

Метод проверяет текущие байты правил и цепочки дескрипторов владельца, включая
предопределённые элементы для `Predefined.xml`. Полного инвентаря и обхода дерева
нет. Нечитаемая, неизвестная или неоднозначная принадлежность даёт
`readOnly:true, unknown:true`; это не утверждение о фактической поддержке объекта.
Чтение по символьной ссылке не даёт разрешения на редактирование.

Проверка выполняется и без предшествующего `didChangeFiles`, поэтому имя ветки,
mtime и поколение дерева не заменяют сверку исходных данных. Результат описывает
данные на момент запроса; это read-only API, а не блокировка будущей записи на диск.
Постраничный API 1.2 и остальные контракты сохранены.


## Редактирование свойств — API 1.6–1.7

Для записи клиент передаёт в `initialize` `apiVersion:{major:1,minor:6}` и
`allowPropertyEdits:true`. Сервер сообщает `propertyEditing:true,readOnly:false`.
Без opt-in, включая клиентов 1.0–1.5, остаются `readOnly:true,propertyEditing:false`.
Разрешение транспорта не отменяет правил поддержки объектов и Workspace Trust.

Методы используют обычные `sessionId`, `projectId`, `generation` и `objectId`:

| Метод | Дополнительные params | Результат |
| --- | --- | --- |
| `metadata/propertyEditing` | — | snapshot, source, profile, writable, fields, readOnlyProperties, undo, redo, addRemove:false |
| `metadata/propertyTypeChoices` | path из fields | choices с key и RU/EN caption |
| `metadata/propertyReferenceChoices` (1.7) | path из fields | choices с value, objectId, metadataKind и RU/EN caption |
| `metadata/previewProperty` | snapshot, path, change | valid, changed, changes — точные замены UTF-8 байтов |
| `metadata/updateProperty` | snapshot, path, change | обычные properties/picture и новый editing |
| `metadata/undoProperty` | snapshot, direction:undo\|redo | обычные properties/picture и новый editing |

Форматы path/change и проверки общие с [CLI для ИИ](metadata-editing.md).
API 1.7 добавляет `schema.kind:"reference"` с `domain` и `nullable`. Клиентам 1.6
сервер не выдаёт новые варианты схем в чтении и ответах на запись; новый метод
для них возвращает method not found. Opt-in `allowPropertyEdits` остаётся обязательным.
Изменившая файл операция публикует `metadata/changed`, no-op — нет. Ответ и события
сохраняют generation/eventSequence. Клиент не повторяет write при stale generation,
таймауте, устаревшем ответе или разрыве соединения: после неопределённого исхода
нужно перечитать файл. `property_committed_refresh_required` явно отличает запись
с последующей ошибкой обновления от отказа до записи. История принадлежит session;
вкладка клиента имеет отдельный lock и сохраняет черновик при внешнем конфликте.
