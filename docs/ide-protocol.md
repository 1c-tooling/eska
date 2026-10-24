# IDE protocol 1.x — базовый контракт и расширения

Статус: спецификация принята 2026-09-18; реализованы `eska ide --stdio` (T70)
и самостоятельный `eska-project serve --stdio` (T92).
Базовый режим поверх T66–T68/T75 не изменяет исходники, manifest и Git.
Допустима запись производного кеша T68. API 1.6 добавляет явное включение записи
существующих свойств; контракт приведён ниже. MCP, LSP, запуск платформы 1С
и зависимость от VS Code в протокол не входят.
Standalone entry объявляет `server.name: "eska-project"` и `selfUpdate: false`;
прежний вход сохраняет `server.name: "eska"`. Оба обслуживает одна реализация.

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
  правила T49 из `crates/eska-presentation/src/encoding.rs`. UTF-8 сохраняется буквально, включая `%`;
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

CLI adapter расположен в `crates/eska-project/src/ide`: отдельные reader и writer, последовательный
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


## Редактирование свойств — API 1.6–1.8

Для записи клиент передаёт в `initialize` `apiVersion:{major:1,minor:6}` и
`allowPropertyEdits:true`. Сервер сообщает `propertyEditing:true,readOnly:false`.
Без opt-in, включая клиентов 1.0–1.5, остаются `readOnly:true,propertyEditing:false`.
Разрешение транспорта не отменяет правил поддержки объектов и Workspace Trust.

Методы используют обычные `sessionId`, `projectId`, `generation` и `objectId`:

| Метод | Дополнительные params | Результат |
| --- | --- | --- |
| `metadata/propertyEditing` | — | snapshot, source, profile, writable, fields, readOnlyProperties, undo, redo, addRemove:false |
| `metadata/propertyTypeChoices` | path из fields | choices с key и RU/EN caption |
| `metadata/propertyReferenceChoices` (1.7) | path из fields | choices с value, objectId, metadataKind, RU/EN caption и необязательным standardAttribute |
| `metadata/propertyValueChoices` (1.8) | path из fields и key из schema.types или propertyTypeChoices для choiceParameter | choices с value и RU/EN caption |
| `metadata/previewProperty` | snapshot, path, change | valid, changed, changes — точные замены UTF-8 байтов |
| `metadata/updateProperty` | snapshot, path, change | обычные properties/picture и новый editing |
| `metadata/undoProperty` | snapshot, direction:undo\|redo | обычные properties/picture и новый editing |

Форматы path/change и проверки общие с [CLI для ИИ](metadata-editing.md).
Скаляр `text` может дополнительно объявлять `domain:"choiceParameterName"`:
имя существующей связи параметров выбора с проверкой синтаксиса и конфликтов
внутри списка. Операция остаётся `change.kind:"text"`; ограничения проверяет
backend независимо от поддержки подсказки клиентом. У обычных строк `domain`
отсутствует. Подробные правила приведены в контракте CLI.
Домен `reference` со значением `ChoiceParameterField` выбирает источник
существующей связи параметров выбора. Список ограничен владельцем/строкой
табличной части и повторно проверяется при записи. Стандартное поле возвращает
`standardAttribute` вместе с `objectId` своего владельца; отдельного узла дерева
для него нет. Состав списка связей сохраняется. Старый UUID-путь можно заменить
вариантом из `propertyReferenceChoices`, но нельзя записать произвольный UUID.
API 1.7 добавляет `schema.kind:"reference"` с `domain` и `nullable`. Клиентам 1.6
сервер не выдаёт новые варианты схем в чтении и ответах на запись; новый метод
для них возвращает method not found. Opt-in `allowPropertyEdits` остаётся обязательным.
Изменившая файл операция публикует `metadata/changed`, no-op — нет. Ответ и события
сохраняют generation/eventSequence. Клиент не повторяет write при stale generation,
таймауте, устаревшем ответе или разрыве соединения: после неопределённого исхода
нужно перечитать файл. `property_committed_refresh_required` явно отличает запись
с последующей ошибкой обновления от отказа до записи. История принадлежит session;
вкладка клиента имеет отдельный lock и сохраняет черновик при внешнем конфликте.

API 1.8 добавляет `schema.kind:"value"` и `change.kind:"value"` для значения
заполнения. Допустимые типы и ограничения публикуются в `schema.types`. Клиентам
1.6–1.7 этот вариант не возвращается ни в чтении, ни в ответах на запись;
`metadata/propertyValueChoices` для них возвращает method not found.

Для существующих параметров выбора `schema.kind:"value"` дополнительно содержит
`domain:"choiceParameter"`. Inline `types` содержит четыре примитивных типа и
текущий распознанный ссылочный тип; полный список запрашивается через
`metadata/propertyTypeChoices`. Выбранный тип передаётся в
`metadata/propertyValueChoices`, затем в обычный `change.kind:"value"`.
Числовые ограничения без `digits`/`fractionDigits` допускают десятичную строку
без квалификаторов разрядности, с общим лимитом размера значения.
Старый клиент может использовать inline-список; произвольные типы при записи
всё равно отклоняются. Имя параметра с доменом `choiceParameterName` адресуется
путём до существующего `app:item` и изменяет только атрибут `name`.
У элементов фиксированного массива сохраняются адреса по occurrence;
контейнер массива не объявляется редактируемым полем. Число и порядок элементов
сохраняются. Полные правила и проверенная область — в контракте CLI.

### API 1.9: переименование

Для записи требуется `allowPropertyEdits:true`; capability `metadataRename:true`
возвращается только после согласования minor 9 и включения записи. Клиенты 1.0–1.8
не получают новые признаки редактирования; методы переименования для них недоступны.

| Метод | Дополнительные params к sessionId/projectId/generation | Result |
| --- | --- | --- |
| `metadata/renamePreview` | objectId, newName | applyAvailable, plan |
| `metadata/renameApply` | objectId, newName, snapshot, reviewedUncertain:bool | properties, picture?, editing, renamed |

`plan` совпадает с [JSON CLI](metadata-editing.md#предварительный-просмотр-переименования):
пути в нём — относительные UTF-8 строки source; диапазоны — смещения UTF-8 байтов.
`snapshot` берётся из preview. Перед записью backend заново строит план и сверяет
снимок. `issues` блокируют запись; `reviewedUncertain:true` подтверждает просмотр
сомнительных мест, которые остаются без изменения. Новые XML, ranges или пути
записи от клиента не принимаются как источник изменений.

В `editing` добавлены `renameAvailable`, `undoRename`, `redoRename`.
Первый признак разрешает предложить переименование текущего объявления;
полная проверка всех исходников выполняется в preview. Последние два признака
показывают, изменит ли ближайший undo/redo имя и другие файлы. До такой записи
клиент проверяет несохранённые документы проекта, а не только текущий XML.

`renamed = {from,to,descendantFrom,descendantTo}` сопровождает ответ на rename
и undo/redo переименования, а также предшествующее ему `metadata/changed`.
Клиент заменяет точный ID `from` на `to`; у ID с префиксом `descendantFrom`
заменяет только этот префикс на `descendantTo`. Все четыре строки задаёт сервер;
клиент не восстанавливает их из имени объекта и не разбирает внутренние сегменты ID.
Переезжают адреса узлов, родительские ссылки и ключи открытых вкладок.
При пропущенном eventSequence клиент перечитывает состояние вместо угадывания
пропущенных переименований.

`rename_invalid_name`, `rename_collision`, `rename_blocked`,
`rename_review_required` и `rename_committed_refresh_required` имеют тот же
смысл, что в CLI. `property_edit_busy` и `property_recovery_required` требуют
перечитывания/восстановления; автоматический повтор записи запрещён.
Отмена запроса до запуска предотвращает запись. После начала mutating request
его фактический результат сохраняется, даже если пришёл `$/cancelRequest`:
завершённая запись не подменяется ошибкой `cancelled`.


### API 1.10: связанные параметры нумерации

Capability `linkedPropertyEdits:true` требует minor 10 и `allowPropertyEdits:true`.
У полей нумератора и ссылки документа `Numerator` появляется `linked:true`;
клиенты 1.0–1.9 не получают эти поля в схеме редактирования.
`editing` дополнен `contextSnapshot` (SHA-256 или null), `linkedObjects`,
`undoLinked`, `redoLinked`. Для связанных `previewProperty`, `updateProperty`
и `undoProperty` нужен точный `contextSnapshot` из отображённой схемы.
Детали проверки и полного списка `files` в preview описаны в
[общем контракте CLI](metadata-editing.md#cli-для-ии-и-скриптов).

Изменение нумератора обновляет соответствующий параметр связанных документов
одной восстанавливаемой операцией. Назначение нумератора переносит все пять
параметров внутри документа. История хранит общую отмену связанных файлов;
внешняя правка любого из них блокирует replay. Клиент сохраняет черновик,
проверяет несохранённые исходники перед записью и не повторяет запрос после
потери ответа. На время запроса связанные записи используют тот же барьер
чтения и длительный таймаут, что и переименование.

### API 1.11: целые значения без потери точности

Capability `unsignedIntegerProperties:true` требует minor 11 и
`allowPropertyEdits:true`. Схема `unsignedInteger` передаёт `min` и `max`
десятичными **строками**, например:

```json
{"kind":"unsignedInteger","min":"0","max":"18446744073709551615"}
```

`field.value` и `change.value` также остаются строками. Клиент не должен
преобразовывать их или границы в IEEE 754 number. Новый вариант используется
для `SessionMaxAge` HTTP- и Web-сервисов: Конфигуратор 8.3.27 сохраняет полные
беззнаковые 64-битные секунды, несмотря на знаковый `ELong` в модели EDT.
Схема `integer` сохраняет прежние числовые границы.

Клиентам с minor 0–10 новые поля исключаются из схемы редактирования с причиной
`readOnlyProperties.reason:"client_version"`; чтение значения сохраняется.
Отрицательные, дробные значения и переполнение отклоняются до изменения XML.
CLI JSON использует тот же вариант схемы без handshake.

### API 1.12: контекст и конфигурация внешнего анализатора

Capability `analyzerConfiguration:true` доступна при запросе minor 12.
Прежние клиенты продолжают использовать Metadata API без анализатора.
Новые методы не являются LSP; только явные prepare/migration requests
запускают опубликованный executable для `--version`, `contract`, `check-config`.
Дерево, свойства и support от него не зависят.

Параметр `start` — абсолютный обратимый path DTO. Необязательный
`application` — имя приложения из ESKA; explicit null отклоняется.

| Request | Дополнительные параметры | Результат |
|---|---|---|
| `analyzer/context` | — | JSON контекста самостоятельного project CLI |
| `analyzer/configStatus` | — | Read-only freshness, ownership, fingerprint и preview token |
| `analyzer/configPrepare` | `analyzer`, optional `timeoutSeconds`, `switchContext` | Тот же JSON подготовки, что в CLI |
| `analyzer/migrationPreview` | `analyzer`, optional `timeoutSeconds` | Полный preview, политика, конфликты, оба предлагаемых текста и token |
| `analyzer/migrationApply` | `analyzer`, `previewToken`, optional `timeoutSeconds` | Подготовленный конфиг и path DTO сохранённого оригинала |
| `analyzer/acquire` | — | Подготовленный актуальный конфиг и `leaseId` |
| `analyzer/release` | Только `leaseId`, без `start` | null |

`analyzer` — абсолютный path DTO выбранного executable, без поиска полной ESKA.
`timeoutSeconds` — целое 1–600, default 60 на subprocess. Все методы —
request-only. Они не требуют metadata generation и могут выполняться до
`workspace/open`; owning root определяется общим resolver.

Ошибка возвращает domain `analyzer_configuration_failed`, details
`{code,data}` совпадают с CLI. Localized/upstream сообщения в JSON не включаются.
Существующие `unknown_session`, `invalid_state` и transport errors сохраняются.
При отклонении `check-config` отдельная notification `analyzer/validationOutput`
содержит исходные `stdout`/`stderr` upstream для output panel; она не входит
в стабильную структуру ошибки и может содержать время и временный путь кандидата.

Один connection удерживает не более одного shared lease. Его можно получить
только для точного актуального config. Другие окна того же контекста получают
shared lease, запись требует exclusive lock. До запуска своего LSP клиент
обязан acquire; перед изменением контекста — остановить LSP и release.
Ложный или уже освобождённый id даёт `unknown_analyzer_lease`.
`workspace/close`, `shutdown` и закрытие процесса освобождают lease.

`$/cancelRequest` прерывает уже работающий validator; EOF отменяет его и
освобождает ресурсы соединения. Публикация прошедшего проверку файла —
commit boundary: пришедшая после неё отмена не переименовывается в неуспех.
Миграция публикует оба конфига под одной exclusive lock с откатом обычной
ошибки второй записи. Не участвующие в блокировке внешние редакторы проверяются
по точным исходным байтам перед заменой.

### API 1.13: снимки и планы правок запросов

При запросе minor 13 capability `queryEditing` содержит `schemaVersion:1`,
`planOnly:true`, `writesOwner:false`, `sourceKinds:["dataCompositionSchema","form","bsl"]`
и лимиты ниже. Capability `editorLineEndings:true` дополнительно разрешает
явный формат новых физических строк в `query/plan`. Эти методы доступны после `workspace/open`. Они не запускают
анализатор, не исполняют SDBL, не записывают XML/BSL и не меняют metadata generation.
`allowPropertyEdits` управляет прежними командами записи свойств; получение
query-плана само по себе не публикует правку.

Для каждого request обязательны `sessionId`, `projectId`, `generation` из текущего
metadata context. Обычные проверки `reopen_required`, `resync_required`,
`stale_generation` применяются до чтения. Все методы request-only.

| Ресурс | Предел |
|---|---:|
| Снимок владельца или текст запроса | 8 MiB UTF-8 |
| Inline `text` или один upload chunk | 128 KiB UTF-8 |
| Сумма снимков документов и зарезервированных uploads | 32 MiB на connection |
| Открытые документы | 16 на connection |
| Документы и uploads вместе | 64 на connection |
| `query/list` | 64 элемента на страницу |
| `query/map` | 64 позиции за вызов |
| Комментарии XML/BSL вне SDBL | 8192 на запрос |

В XML дополнительно ограничены 1024 запроса на документ, суммарные карты
65 536 участками и суммарные повторяемые identity-строки 8 MiB; один dataset path
не больше 65 536 байт. Парсер ограничен 250 000 nodes и глубиной 64.
BSL scanner ограничен 131 072 токенами и 65 536 частями/участками карты.
Базовые transport limits 1 MiB/frame и 4 MiB queued bodies остаются прежними.

Большой буфер передаётся частями:

* `query/bufferBegin`: `{...context,bytes,snapshot}`. `snapshot` — SHA-256 полных
  UTF-8 bytes, 64 lowercase hex digits. Ответ: `{bufferId,receivedBytes:0,ready}`.
  Место резервируется сразу; пустой буфер готов только с SHA-256 пустой строки.
* `query/bufferAppend`: `{...context,bufferId,offset,text}`. `offset` — ровно
  уже принятое число байтов; chunk — непустая UTF-8 строка до 128 KiB. Ответ:
  `{bufferId,receivedBytes,ready}`. При завершении проверяется SHA-256; неверный
  upload удаляется. Частичный буфер использовать нельзя.
* `query/bufferClose`: `{...context,bufferId}` → `{released:boolean}`.

Uploads принадлежат создавшим их session/member/generation. Чужой или ещё
не готовый id даёт `query_failed/unknown_buffer`; неверный offset —
`upload_offset`; неверный SHA-256 — `snapshot_mismatch`. Буфер не потребляется
чтением или планированием: клиент освобождает его явно.

`query/open` принимает `{...context,node,source:Path,version,application?,text?,bufferId?}`.
`source` — путь относительно source выбранного member; backend сверяет его
с источниками `node`, а не с расширением файла. `version` — положительный
десятичный u64 в строке, версия редакторского буфера. `text` и `bufferId`
взаимоисключающие; при отсутствии обоих читается диск. `application` выбирается
общим resolver из `eska.toml`; выбранный member обязан входить в это приложение.

Ответ: `{documentId,version,snapshot,diskSnapshot,source,sourceKind,application,
contextFingerprint,writable,limitedContext,...context}`. `application` содержит
разрешённое имя контекста, либо null для standalone. `snapshot` относится к
полному исходному буферу, `diskSnapshot` — к дисковой копии под ним. Fingerprint
включает реальные manifests, корневые XML и состав приложения через тот же
resolver, что использует конфигурация анализатора; готовый derived config не требуется.
Сам `bsl-analyzer.toml` не читается и не входит в query fingerprint: это отдельный
снимок Core `application::resolve_snapshot`, а не token производного конфига.
Один owner path в одном member нельзя открыть дважды (`already_open`).

Backend сохраняет полные исходные bytes буфера. Координаты относятся именно
к нему: BOM, если он передан, считается символом. Если `TextDocument.getText()`
редактора не содержит дисковый BOM, клиент передаёт этот точный текст при открытии,
а не применяет координаты сырой дисковой копии к другому буферу. Сохранность BOM
при последующем сохранении редактором проверяется отдельно в UI-приёмке.

Для `query/list`, `query/read`, `query/map`, `query/plan` дополнительно обязательны
`documentId`, `version`, `snapshot`, `contextFingerprint` из `query/open`.
Документ привязан к metadata generation; после refresh клиент закрывает его
и явно получает новый снимок, сохраняя свой черновик. Проверяются версия и полный
буфер, UUID, дисковые bytes и текущая support policy без зависимости от watcher.
Кроме UUID backend сохраняет путь и SHA-256 всех bytes XML-дескриптора владельца.
Перед чтением/планированием проверяется тот же дескриптор через ограниченный
resolver с проверкой containment. Даже изменение пробелов или комментариев
при прежнем UUID даёт `query_failed/owner_changed`. Это распространяется на
дочерние дескрипторы, не входящие в application fingerprint, и не требует
повторной реализации hash-проверки адаптером другой IDE. Новый `query/open`
создаёт новую исходную привязку; он сам по себе не возобновляет прежнюю сессию.
Приложение повторно разрешается до и после запроса. Изменение контекста не
перепривязывает документ автоматически.

Query identity имеет одну из форм:

```json
{"kind":"dataSet","path":["Union","DataSet"]}
{"kind":"dynamicList","name":"Список","id":"1"}
{"kind":"bslLiteral","range":{"start":120,"end":340}}
```

Dataset path включает union-предков. Имя вкладки и XML/BSL byte range не являются
identity вне проверенного owner snapshot. BSL range backend получает scanner'ом
и при повторном обращении требует совпадения всего литерала.

`query/list`: `{...binding,offset?,limit?}` → `{queries:[{identity,sourceRange}],nextOffset,...}`.
Это XML-запросы в порядке исходника: только `DataSetQuery` в namespace СКД и
`DynamicList` с `ManualQuery=true|1` и сохранённым `QueryText` в namespace формы.
Автоматический список без текста не получает синтезированного запроса.

`query/read`: `{...binding,query?:Identity,at?:{line,character}}`.
Нужно ровно одно из `query` и `at`; `at` — исходная BSL-позиция в UTF-16.
Ответ содержит `query:{identity,text,sourceRange,sourceRangeUtf16,map,comments}`.
`text` — чистый SDBL; весь пакет и расширения `{…}` остаются целыми. BSL поддерживает
прямые значения литералов при присваивании/возврате и текст конструктора
`Новый Запрос`/`New Query`, включая внешние скобки. Конкатенации, соседние отдельные
литералы, преобразования и неоднозначные вызовы дают `unsupported_expression`;
код и имена переменных не вычисляются.

`map:{encoding:"utf-8",spans:[{query:{start,end},source:{start,end},direct}],anchors}`.
Обычные bytes объединены в direct spans; entities, удвоенные кавычки и
нормализованные CRLF имеют отдельные интервалы. Внутренность escape и пропущенные
комментарии/отступы не округляются к соседнему токену. `anchors` содержит
`startBefore`, `startAfter`, `endBefore`, `endAfter`, в том числе для пустого текста
и последней пустой строки. `comments:[{comment,source,queryOffset,text}]` хранит
комментарии XML/BSL отдельно от SDBL и сохраняет их исходное написание.

`query/map`: `{...binding,query:Identity,direction:"queryToSource"|"sourceToQuery",
positions:[{line,character}],affinity?:"before"|"after"}`.
Все входные/выходные позиции — UTF-16; ответ `positions` сохраняет порядок.
Пропущенная source syntax даёт null, неверная позиция или половина surrogate pair —
`invalid_position`. Affinity выбирает сторону source-only gap; default `after`.
Этот метод работает с зафиксированным оригиналом владельца.

`query/plan`: `{...binding,query:Identity,text?,bufferId?,comments:[{comment,queryOffset}],lineEnding?:"lf"|"crlf"}`.
Нужно ровно одно значение `text`/`bufferId`. `comments` — массив явных решений:
`queryOffset` — UTF-8 позиция в новом тексте, null означает явное удаление.
Это обязательное поле решения; его отсутствие не означает удаление. Пропущенные
решения сохраняют комментарии на однозначных местах неизменённого текста;
неоднозначность даёт `comments_require_decision`. Для BSL назначение возможно
только перед строкой после LF, внутри многострочного литерала.

Редактор передаёт `lineEnding` из модели владельца, когда сервер объявляет
`editorLineEndings:true`. Новые физические строки XML и BSL используют этот
формат: VS Code не должен дополнительно нормализовать план и менять его хэш
или projected map. В том числе это относится к первой вставке перевода строки
в однострочный источник. Содержимое XML character references не меняется,
source comments сохраняют исходные bytes. No-op не нормализует владельца.
Без поля действует прежнее планирование; наличие capability проверяется
перед использованием поля. Чистый SDBL XML передаётся с LF; CR в значении
не используется для управления физическим стилем XML.

Ответ: `{noOp,outputSnapshot,replacements,projectedQuery,...binding}`.
`replacements:[{range,rangeUtf16,text}]` адресуют исходный owner snapshot;
`projectedQuery` и его карта адресуют результат применения этих правок в памяти.
Его новый BSL range нельзя использовать как identity оригинала до применения
и повторного открытия документа. Backend повторно извлекает значение после
замены и сверяет текст/комментарии. No-op возвращает пустой список правок и
побайтно прежний источник. XML использует существующий scalar patch; при правке
CDATA может стать escaped text, XML вокруг значения не сериализуется. Для BSL
чистый текст использует LF, CRLF черновика нормализуется в LF; comment offsets
задаются в этом нормализованном тексте. `lineEnding` управляет только новым
source-синтаксисом, остальная часть владельца сохраняется побайтно.

Клиент проверяет актуальные version/snapshot/context перед `WorkspaceEdit` и
применяет план только к этому буферу. План не является командой сохранения файла
и не заменяет блокировку BSL-владельца в T102. Запрещённый источник можно читать,
но план для него даёт `read_only`; последующее снятие запрета требует нового open.

`query/close`: `{...context,documentId}` → `{released:boolean}`. Оба метода close
работают при устаревшей generation и не требуют актуального приложения; session
и member должны оставаться владельцами id. `workspace/close`, `shutdown`, EOF
освобождают все retained snapshots. Поздняя отмена не скрывает acknowledgement
завершённого begin/append/open/close; отмена read/map/plan не публикует правку.

Domain `query_failed` содержит только `details.reason`: `too_large`, `invalid_xml`,
`unsupported_structure`, `invalid_literal`, `unsupported_expression`, `not_found`,
`invalid_position`, `invalid_value`, `conflict`, `comments_require_decision`,
`read_only`, `owner_changed`, `stale_version`, `context_changed`, `context_member_mismatch`,
`already_open`, `unknown_document`, `unknown_buffer`, `upload_offset`, `snapshot_mismatch`.
`query_context_unavailable` содержит прежний `{code,data}` resolver/config error.
`resource_limit` называет конкретный ресурс и его предел. JSON не зависит от locale.

### API 1.14: общее восстановление черновиков

Опциональная capability `queryRecovery` доступна при handshake 1.14:
`{schemaVersion:1,writesOwner:false,storageVersion:1,maxRecords:16,
maxRetainedBytes:33554432,maxMetadataBytes:1048576,maxTextBytes:8388608,
maxChunkBytes:131072,maxUploads:1,compareRevision:true}`.
Клиенты 1.13 сохраняют прежние query RPC; новые recovery-методы им недоступны.
Реализация хранилища и проверка данных находятся в ESKA, без API конкретной IDE.
Рабочий адаптер Explorer использует этот RPC вместо клиентского файлового
хранилища. Старый backend 1.12–1.13 сохраняет дерево и обычный BSL-клиент,
но для этого редактора запросов требуется queryRecovery 1.14.
Перенос восстановления не объявляет завершёнными модель защиты, блокировку
BSL-владельца и остальные требования T102.

Все методы — request-only и доступны после initialize без workspace/open.
`sessionId`, `projectId`, `generation` не используются. Журнал принадлежит
connection: черновик можно сохранить и прочитать при requiresRefresh/requiresReopen,
после workspace/close или при ошибке открытия проекта. Это не разрешение применить
его к исходнику. Каждый метод после open использует и возвращает `storeId`.

`query/recoveryOpen`: `{directory:Path}`. Directory — абсолютный
приватный каталог адаптера вне root/source всех открытых проектов и известного
ESKA-корня рабочего каталога процесса. Известный корень определяется наличием
eska.toml при старте без разбора его содержимого, поэтому невалидный manifest
не блокирует восстановление. Местоположение повторно проверяется перед записью
и удалением после изменения сессии метаданных.
Fallback в проект, переходы к родителю и symlink-компоненты запрещены.
Отсутствующий каталог при чтении не создаётся. Один connection держит один
каталог; смена требует close. Повторный open без активного upload перечитывает
файлы и выдаёт новый непрозрачный `storeId`.

Ответ: `{storeId,records:[Header],damaged:[id],limited:boolean}`.
`damaged` перечисляет непринятые записи: неверный формат, alias или превышение
квот; файлы сохраняются. `limited:true` отдельно указывает неполный результат
из-за лимита. Этот результат не считается отсутствием сохранённых черновиков.
Header: `{id,revision,metadata,parts}`. `revision` — SHA-256 точных подтверждённых
bytes файла, включая прежний порядок JSON полей клиента T101.
`parts` имеет ключи `metadata`, `ownerText`, `base`, `draft`; значение каждого —
`{bytes,snapshot}` с длиной UTF-8 и SHA-256 полного компонента.

Metadata использует binding checkpoint v1: `schemaVersion:1`, canonical UUID v4
`id`, непрозрачный `projectKey`, `node:NodeId`, `source:Path`, `descriptor:Path`,
`descriptorSnapshot`, `diskSnapshot`, `ownerSnapshot`, положительный u64
`ownerVersion` строкой, `contextFingerprint`, `application:string|null`,
`identity:Identity` и `writable:boolean`. Лишние поля checkpoint отклоняются.
`ownerText` содержит полный исходный буфер, `base` — исходный SDBL, `draft` —
последний черновик. UTF-8, BOM, NUL и незавершённый текст сохраняются буквально.
Поддерживаемый предыдущим клиентом JSON/base64 storageVersion 1 читается без
перезаписи при загрузке. Запомненные paths и writable — данные восстановления;
перед применением требуется заново проверить источник, контекст и права.

`query/recoveryRead`: `{storeId,id,revision,part,offset}`.
Part — один из четырёх ключей выше; offset задаётся в UTF-8 bytes и не может
разрезать символ. Ответ: `{id,revision,part,offset,text,nextOffset,complete}`.
Один chunk не превышает 128 KiB UTF-8. Заголовок и все chunks адресуют одну
подтверждённую ревизию; чтение не подменяет её записью другого окна.

`query/recoveryBegin`: `{storeId,id,expectedRevision:string|null,parts}`.
Null означает явное создание новой записи; строка — точную предыдущую revision.
Для совместимости пропущенный expectedRevision также означает null.
Все четыре descriptions обязательны. Backend резервирует ограниченный upload
и переиспользует неизменённые подтверждённые компоненты без копирования владельца.
Ответ: `{updateId,received:{metadata,ownerText,base,draft}}`, значения — числа
принятых bytes. Для переиспользованной части уже принята вся заявленная длина.

`query/recoveryAppend`: `{storeId,updateId,part,offset,text}`.
Принимает непустой chunk до 128 KiB, строго по следующему offset, и проверяет
полный hash завершённого компонента. Ответ:
`{updateId,part,receivedBytes,ready}`. Неверный hash освобождает весь upload;
ошибка offset его не публикует. Сами begin/append файлов не создают.

`query/recoveryCommit`: `{storeId,updateId}` → новый Header.
Только завершённый upload с правильным полным ownerSnapshot становится записью.
ESKA проверяет текущую дисковую revision и общую квоту под короткой блокировкой
хранилища, пишет отдельный файл и атомарно заменяет checkpoint. На Unix новые
каталоги имеют mode 0700, файлы — 0600. Блокировка хранилища освобождается ОС
при закрытии handle/процесса; это отдельный механизм от защиты исходного редактора.
Два backend-процесса используют общий lock inode, который не удаляется.
Прямая внешняя запись, не участвующая в блокировке, проверяется перед rename;
операция не объявляется OS compare-and-swap для такого стороннего writer.

`query/recoveryCancel`: `{storeId,updateId}` → `{released}`.
Освобождает только staging в памяти и не удаляет подтверждённый checkpoint.
`query/recoveryRemove`: `{storeId,id,revision}` → `{released}`.
Удаление требует текущей revision и отменяет upload той же записи.
Поздний commit старого updateId не восстанавливает удалённый черновик.
Повторное удаление уже отсутствующей в handle записи отвечает released false.
Явный begin с null создаёт новую запись; адаптер не преобразует запоздалое
обновление закрытой сессии в такое создание.

`query/recoveryClose`: `{storeId}` → `{released:true}`.
Закрытие store, shutdown и EOF освобождают retained state; checkpoint
на диске сохраняется. Workspace/close закрывает только метаданные; журнал остаётся
доступным до своего close или завершения connection. Незавершённые uploads не применяются. После изменения
состояния поздняя отмена запроса не скрывает acknowledgement. При shutdown
адаптер сначала дожидается подтверждения своих checkpoint-запросов.

В domain `query_failed` добавлены reasons `unknown_store`, `store_already_open`,
`unknown_update`, `upload_busy`, `upload_incomplete`, `recovery_invalid`,
`recovery_path`, `recovery_in_project`, `recovery_conflict`, `recovery_busy`,
`recovery_io`, `recovery_limit`; используются также прежние `upload_offset` и
`snapshot_mismatch`. `resource_limit` обозначает `query_recovery` или
`query_recovery_part`. Ошибки не содержат исходного текста или локальных IO messages.

### API 1.15: общая модель сессии запроса

Опциональная capability `querySessions` доступна при handshake 1.15:
`{schemaVersion:1,writesOwner:false,checkpointStorageVersion:1,maxSessions:16,
maxRetainedBytes:33554432,nativeApplicationReceipt:true,explicitResume:true,
preparedStateDurable:false,bslEditing:false,sourceKinds:["dataCompositionSchema","form"]}`.
Клиенты 1.14 сохраняют прежнее поведение; методы session для них недоступны.
Режим 1.15 проверен прямым клиентом `eska-project`, без API конкретной IDE.
Текущий рабочий Explorer требует устойчивый вариант 1.16 ниже.

Core `QuerySession` владеет полным owner snapshot, base, draft, identity,
выбранным приложением и fingerprint, проверками версии/дескриптора/прав,
явным возобновлением и одной операцией применения. Backend никогда не пишет
файл владельца. Адаптер передаёт факты native-модели, показывает результат,
применяет проверенный план штатной отменяемой правкой и подтверждает его.
Это не подтверждение нативной блокировки BSL-владельца: sessionOpen для BSL
возвращает `owner_lock_required`, даже когда обычное чтение литерала доступно.

Методы request-only используют общий metadata context
`{sessionId,projectId,generation}`. Изменение generation само по себе не меняет
identity: после resync клиент передаёт актуальный context. Полный источник,
дескриптор и application fingerprint всё равно проверяются заново перед планом.
`querySessionId` — непрозрачный handle текущего процесса; `revision` сессии —
положительный u64 строкой, отдельный от native version и SHA-256 checkpoint.
Одна активная сессия на физический путь владельца в member; повторное открытие
возвращает `already_open` с существующими `querySessionId` и recovery `id`.

`query/sessionOpen`: `{...context,documentId,storeId,id,query:Identity,
format:"editor"|"source",userReadOnly:boolean}`.
Document получают через query/open 1.13 из текущего полного буфера владельца.
`id` — новый canonical UUID v4; `storeId` — открытый recovery store 1.14.
Успех атомарно сохраняет полный начальный checkpoint, поглощает document handle
и возвращает SessionHeader. При ошибке document остаётся доступным.
`editor` означает, что native-текст не содержит дисковую UTF-8 signature BOM;
`source` — что он содержит точный исходный текст, включая signature при её наличии.
Остальные символы текста не нормализуются.

SessionHeader: `{querySessionId,storeId,id,revision,resumed,applyPending,
draftWritable,format,protection:{capturedSourceWritable,userReadOnly,ownerLock,sourceRestriction},
record:Header}`. `record` — прежний recovery Header; полные компоненты читаются
через recoveryRead. `capturedSourceWritable` — запомненная доступность источника,
а не разрешение пропустить повторную проверку. `ownerLock` пока null в RPC:
он относится к подтверждению нативной блокировки BSL. Дополнительный
`sourceRestriction:string|null` обозначает собственное ограничение записи
в backend; это положительный token текущей project session, который может
повториться в другом процессе и не предоставляет права записи по числу.
В старых сборках поле отсутствует. Активная сессия удерживает исходник и
его XML-дескриптор: обычный query/plan, обновление свойств и их Undo/Redo,
связанные правки и затрагивающее их переименование отклоняются. Подготовка
через владеющую query/sessionPrepare по-прежнему проверяет support policy.
Переименование сообщает issue `source_session_active`, read-only scalar —
прежний `property_read_only`/`query_failed` с `reason:read_only`.
Восстановление структурного журнала с неизвестным набором записей требует
освободить все активные ограничения проекта. Обновление метаданных и
переименование других объектов не снимают ограничение. Close, удаление
checkpoint и уничтожение последнего владеющего состояния освобождают только
его ограничение; flags пользователя и support policy не меняются.
Это защита операций текущего backend, без глобальной блокировки файла или
запрета ввода в IDE. Изменения других процессов всё равно проверяются по снимкам.

Изменить draft можно через recoveryBegin/Append/Commit: для активной
сессии metadata, ownerText и base должны совпадать с подтверждёнными данными,
а expectedRevision — с её текущим checkpoint. Обновление draft продвигает revision
сессии и не меняет разрешение на применение. Лимит памяти учитывает также
ожидающий применения output и прежний draft, если пользователь уже ввёл новый.

`query/sessionRestore`: `{...context,storeId,id,revision,format}`.
`revision` здесь — SHA-256 существующего checkpoint. Backend проверяет его
привязку к проекту, member, владельцу и исходному запросу; выдаёт SessionHeader
с `resumed:false`. Данные checkpoint не предоставляют права применить правку.
Для восстановления source-сессии нужен доступный metadata context; чтение
неприменяемых данных recovery по-прежнему доступно при невалидном manifest.

`query/sessionGet`: `{...context,querySessionId}` → SessionHeader.
Чтение собственного состояния допускает устаревшую generation и metadata flags.

OwnerFacts: `{version:string,snapshot:string,isDirty:boolean,
userReadOnly:boolean,application:string|null}`. Snapshot — SHA-256 точного
native-текста в выбранном format, version — текущий положительный счётчик IDE.
Application обязателен: null означает текущий выбор по умолчанию, строка —
текущее явно выбранное приложение. Backend разрешает этот выбор заново;
не следует незаметно применять запрос в ранее сохранённом контексте.

`query/sessionResume`: `{...context,querySessionId,revision,owner:OwnerFacts}`.
Требует явного действия пользователя, полного совпадения исходного буфера,
дескриптора, контекста и разрешений. Новый native version после перезапуска
допустим. Если XML-владелец был отдельно сохранён, чистая модель должна точно
совпадать с текущим диском с учётом BOM; dirty-модель не маскирует дисковый конфликт.
Успех сохраняет обновлённый checkpoint до выдачи SessionHeader.

`query/sessionPrepare`: `{...context,querySessionId,revision,owner:OwnerFacts,
comments:[{comment,queryOffset}],lineEnding?:"lf"|"crlf"}`.
Планирует текущий подтверждённый draft; comments и lineEnding имеют семантику
query/plan 1.13. Ответ — SessionHeader с `applyPending:true` и
`plan:{applyToken:string,saveOwner:boolean,noOp,outputSnapshot,replacements,projectedQuery}`.
Формат правок и projectedQuery прежний. Второй prepare даёт `apply_busy`.
`saveOwner:true` бывает только для изменённого XML из первоначально чистой
native-модели; dirty-владелец не сохраняется автоматически. No-op не сохраняет файл.
План привязан к исходным version/snapshot: адаптер повторно проверяет модель
непосредственно перед штатной правкой. До выдачи плана ESKA проверяет также
дисковую revision checkpoint, включая изменение другим backend-процессом.

`query/sessionApplied`: `{...context,querySessionId,applyToken,owner:OwnerFacts}`.
Подтверждает точный outputSnapshot и увеличение native version после реальной
правки. Для saveOwner дополнительно требуется точное сохранённое содержимое
диска; для dirty-владельца — прежний диск либо отдельно сохранённый точный output
с чистой native-моделью. Привязка, контекст и права проверяются заново.
Успех атомарно публикует новые owner/base и возвращает SessionHeader;
draft, изменённый после prepare, сохраняется. Второе подтверждение того же токена
отклоняется. Ошибка CAS не продвигает owner/base в памяти.
Метод допускает устаревшую generation и flags после собственной записи XML,
но это не отменяет перечисленных проверок. В ответе приходит текущая generation.

`query/sessionAbort`: `{...context,querySessionId,applyToken}`.
Освобождает только ожидающую операцию в памяти, оставляет checkpoint и требует
нового явного resume. Допускает устаревшие metadata flags и checkpoint;
поздняя отмена чужого/завершённого токена не принимается.
`query/sessionClose`: `{...context,querySessionId}` →
`{released,releasedOwnerLock:string|null,userReadOnly:boolean|null}`.
Освобождает только свою сессию/ограничение, сохраняя независимый userReadOnly
и checkpoint. При отсутствии сессии оба значения null. Close допускает stale
generation. RecoveryRemove освобождает связанную source-сессию; повторный
recoveryOpen, recoveryClose, workspace/close, shutdown и EOF инвалидируют handles.

`preparedStateDurable:false` означает, что точный незавершённый output и
применяемая версия draft не журналируются отдельно. При остановке процесса
сохраняются исходные owner/base и последний подтверждённый draft, который уже
может отличаться от применяемого. Устойчивый вариант доступен в API 1.16 ниже;
рабочий адаптер использует устойчивый вариант 1.16.
Восстановление никогда автоматически не применяет исходник.

Добавлены reasons `unknown_query_session`, `resume_required`, `owner_lock_required`,
`user_read_only`, `stale_draft`, `apply_busy`, `unknown_apply`, `invalid_receipt`,
`closed_session`, `session_limit`, `owner_busy`; действуют прежние source/context/recovery errors.
`resource_limit` использует `query_sessions` или прежний `query_snapshots`.
JSON-схема и машинные причины одинаковы в ru-RU и en-US.

### API 1.16: устойчивый журнал применения

При handshake 1.16 `querySessions` сохраняет прежнюю форму и объявляет
`preparedStateDurable:true,preparedStorageVersion:2`. `checkpointStorageVersion:1`
относится к прежней проекции binding и четырём обычным recovery parts.
Методы сессии сохраняют имена и параметры. Клиент 1.15 получает прежнюю
capability с false и прежнюю подготовку/отмену в памяти. `bslEditing:false`
действует в обоих режимах; эти проверки не принимают нативную блокировку IDE.
Рабочий Explorer использует `query/session*` для XML вместо клиентского lifecycle
и прежнего `query/plan`. Core проверяет источник, контекст и восстановление;
адаптер передаёт факты Native-модели, применяет план и подтверждает результат.
Новый Native-ввод во время подтверждения переносится на подтверждённые owner/base
общей очередью recovery. Оборванное применение остаётся в журнале до явного resume.
[Проверка установленного адаптера](measurements/t102-session-adapter.json).

В режиме 1.16 sessionPrepare атомарно сохраняет исходные owner/base/draft
вместе с точным output и декодированным применяемым запросом **до** выдачи плана.
Успешный ответ содержит новую `record.revision`; последующие draft uploads
используют именно её. Применяемый запрос не заменяется новым draft во время
ожидания IDE. Лимит общего хранилища учитывает оба дополнительных текста,
включая intent без активного source handle. Source save по-прежнему выполняет
адаптер; backend пишет только приватный журнал.

Record Header дополнен необязательным
`prepared:{applyToken,format,saveOwner,noOp,parts:{output,appliedDraft}}`.
Descriptions имеют прежние `{bytes,snapshot}`. Обычный `parts` содержит
те же четыре ключа; поля binding metadata v1 не меняются.
Checkpoint без намерения остаётся storageVersion 1. С намерением файл использует
storageVersion 2 и отдельный `prepared` envelope с canonical base64 текстами
или локальными ссылками на совпадающие обычные части этой же записи.
Ссылки не указывают на paths, другие записи или другие ссылки. Общие копии
не раздувают файл за существующий предел сканирования хранилища 64 MiB.
Обе версии читаются без перезаписи. После проверенной сверки или отмены
запись возвращается к v1; предыдущие поля и последний draft сохраняются.
Неизвестный/повреждённый envelope сохраняется и отмечается в damaged.

`query/recoveryPreparedRead`: `{storeId,id,revision,part:"output"|"appliedDraft",offset}`.
Ответ имеет прежние `{storeId,id,revision,part,offset,text,nextOffset,complete}`.
Чтение request-only, до 128 KiB по границам UTF-8, доступно после initialize
без workspace/open, в том числе при невалидном manifest. Чужая revision
отклоняется. Отсутствие намерения даёт `unknown_apply`.

SessionRestore с намерением возвращает `resumed:false,applyPending:true`.
SessionPrepare до явного resume даёт `resume_required`; sessionApplied
не заменяет действие возобновления и не принимает прежний токен автоматически.
SessionResume сверяет актуальные права, контекст, полный дескриптор и одно из:

- Native snapshot равен исходному owner: подготовленная правка не повторяется,
  намерение снимается, исходные owner/base и последний draft сохраняются.
- Native snapshot равен точному output: backend принимает уже выполненную
  правку как новый owner/base и сохраняет более новый draft. Счётчик IDE после
  перезапуска может начаться заново. Dirty output с прежним диском, включая
  остановку между правкой чистого XML и его сохранением, остаётся dirty и
  не сохраняется backend автоматически. Для чистой модели диск обязан точно
  соответствовать output с учётом BOM.

Любое другое содержимое оставляет конфликт и всё намерение. Успешное resume
публикует очищенный checkpoint до разрешения следующего плана. Обычное
recoveryBegin/Append/Commit при наличии intent может заменить только draft;
binding, owner и base остаются прежними даже без активной source-сессии.
SessionOpen другого handle для того же известного владельца даёт
`pending_apply` с recovery id: сначала требуется сверить или явно отбросить intent.

SessionAbort в режиме 1.16 снимает намерение через дисковый CAS и переводит
сессию в paused; ошибка IO/CAS не объявляется успешной очисткой. SessionClose
всегда может освободить собственный handle, оставляя checkpoint/intent для
восстановления. Проверка независимых пользовательских ограничений сохраняется.
RecoveryRemove без дополнительного решения возвращает `pending_apply`.
Явный `discardPrepared:true` в recoveryRemove разрешает удалить весь checkpoint
по точной revision, в том числе без metadata context; исходник не изменяется.
Этот флаг и recoveryPreparedRead доступны только клиенту 1.16.

При recoveryOpen клиентам 1.14–1.15 записи с intent не выдаются как обычные
черновики: их id попадают в damaged как неподдержанная версия данных,
файлы остаются прежними. Это не отсутствие сохранённой работы. Остальные v1
записи и методы прежних версий сохраняют поведение. T101-клиент, читающий
файлы самостоятельно, также не принимает storageVersion 2 за v1.

### API 1.17: общее состояние защиты и снимки исходников

Опциональная capability `sourceProtection` доступна при handshake 1.17:
`{schemaVersion:1,writesOwner:false,sessionOwnerPriority:true,binarySnapshots:true,
maxSnapshotBytes:8388608,maxSnapshots:16,maxRetainedBytes:33554432,
sharedQueryBudget:true,maxChunkBytes:131072,maxPaths:128,maxRetainedEntries:64}`.
Старые клиенты не получают capability и вызывают новые методы с Method not found.
Бюджет 32 MiB и 64 entries общий с query uploads, documents и sessions;
неизменяемые source views дополнительно ограничены 16 handles. Source API
не запускает анализатор, индекс, полный обход или запись файла.

Запросы используют `{sessionId,projectId,generation}` прежнего metadata context.
Admission требует свежую generation и отсутствие refresh/reopen flags.
Чтение уже удерживаемого снимка и cleanup допускают прежнюю generation/flags;
контекст процесса, metadata session и member всё равно проверяется. Замена
процесса, workspace/close, shutdown и EOF уничтожают handles.

`source/protection`: `{...context,files:[{path:Path,userReadOnly:boolean}],
enforceSupport:boolean}`. От 1 до 128 source-relative файлов. Успех возвращает
обычный context envelope и `files:[Protection]` в том же порядке.
`Protection` содержит `path,objects,supportReadOnly,supportReason,supportUnknown,
mixed,supportEnforced,userReadOnly,filesystemReadOnly,exists,writable,diagnostics,
restrictions:[{id:string,source:Path}]`.
Причина поддержки использует FileReason из supportFiles; ограничения обозначают
каждую активную сессию, удерживающую физический файл или его дескриптор.
`writable` вычисляет core, включая известность владельца и состояние файла.
`userReadOnly` — независимое ограничение, переданное адаптером, без получения
или изменения нативного read-only override самим backend.

`enforceSupport:false` соответствует существующему пользовательскому отключению
обычной защиты поддержки для native-операций. Оно не снимает временное владение,
ограничение пользователя, отсутствие исходника или файловый read-only.
Query/plan и query/sessionPrepare не получают такого переключателя: их support
policy остаётся обязательной. Результат protection — свежие факты для preflight,
а не долговременное разрешение или exemption по числу id.

`source/snapshotOpen`: `{...context,path:Path,
native?:{version:string,userReadOnly:boolean,text?:string,bufferId?:string}}`.
Native input требует положительный u64 version и ровно один text/bufferId.
Inline text ограничен 128 KiB UTF-8. Больший буфер использует `source/bufferBegin`,
`source/bufferAppend`, `source/bufferClose`: формы и правила прежнего query buffer
API 1.13, включая byte offsets, SHA-256 и ровные UTF-8 границы. Это aliases
общего bounded upload storage. Успешное snapshotOpen поглощает только переданный
buffer; при ошибке он остаётся доступен для явного close.

Если path удерживает активная QuerySession, backend выбирает её полный owner
snapshot, включая несохранённые изменения, даже при переданном более новом
native input. Иначе выбирается переданный native-буфер, затем точные bytes диска.
Чужой/неизвестный источник, traversal и alias за пределы source не становятся
file-reader capability. Native input не создаёт новую исходную идентичность.

Ответ: обычный context envelope и `{snapshotId,path,bytes,snapshot,
origin:"disk"|"editor"|"querySession",version:string|null,protection:Protection}`.
Snapshot — SHA-256 фактических удерживаемых bytes; disk version null. Снимок
не декодирует binary, не нормализует BOM/newlines и не даёт права записи.
Query owner переиспользует immutable text; view не удерживает его write lease.
Закрытие query-сессии поэтому не мешает дочитать ранее открытый снимок.

`source/snapshotRead`: `{...context,snapshotId,snapshot,offset,bytes}`.
Offset — byte offset; bytes от 1 до 131072. Ответ без metadata envelope:
`{snapshotId,snapshot,offset,data:string,nextOffset,complete:boolean}`.
Data — canonical base64 точного chunk. Здесь нет ограничения UTF-8 границы:
по частям передаются также binary и части многобайтового символа. Данные
не перечитываются с диска между chunks. Offset в конце допустим и возвращает
пустой complete chunk. Чужая или неверная snapshot hash отклоняется.

`source/snapshotClose`: `{...context,snapshotId}` → `{released:boolean}`.
Cleanup допускает stale generation, но не чужой member. Повторный close
возвращает false. View handles не хранятся на диске и не восстанавливают
разрешения на применение. Прежний private recovery остаётся отдельным API 1.14.

Добавлен `source_view_failed` с reasons `unknown_source`, `missing`,
`invalid_editor_state`, `unknown_snapshot`, `snapshot_mismatch`;
действуют прежние source/context/upload errors. Resource limit использует
`source_snapshot` для одного view или общий `query_snapshots`.
Source bytes и локальные IO diagnostics в ошибках не возвращаются.

Рабочий SDK проверяет DTO, offsets, canonical base64 и полный hash, затем
освобождает handles. Языковые правки Explorer получают общий writable в ESKA
и заново проверяют native versions и transport freshness перед передачей IDE.
Запрос, способный изменить source ownership, заранее инвалидирует старый
preflight в своём процессе. Нативная блокировка BSL-владельца этим не принимается.
Провайдер `eska-protected:` использует тот же source reader. Адаптер передаёт
существующий dirty native-буфер и версии, не читает содержимое файла сам и
следит за событиями IDE. Завершение source-сессии обновляет представление;
view не удерживает её ограничение. Это не подтверждение блокировки file URI.

### API 1.18: защищённый просмотр без метаданных

Capability `sourceProtection.rootedPreviews:true` доступна клиенту 1.18;
клиенту 1.17 возвращается false. Она сохраняет восстановление защищённой вкладки
при отсутствующем или невалидном manifest и чтение нераспознанных файлов.
Чтение выполняет ESKA, обходного file reader в расширении нет.

`source/previewOpen`: `{path:Path}`. Path задаётся относительно **рабочей папки
процесса serve**, которая фиксируется при его создании. Запрос не выбирает
другую корневую папку. Метод доступен после initialize, до workspace/open,
после его ошибки и после workspace/close. Метод не использует native-буфер,
query identity и metadata context. Проверяются обычные относительные
компоненты пути, canonical containment, regular file и предел 8 MiB.
Абсолютный путь, traversal и symlink за пределы корня отклоняются.

Ответ имеет поля обычного SourceView без metadata envelope: snapshotId, path,
bytes, SHA-256 snapshot, `origin:"disk"`, `version:null` и Protection.
В таком Protection `objects:[]`, `supportUnknown:true`, `supportReadOnly:true`,
`writable:false`, diagnostics содержит `source_preview_only`. Права физического
файла остаются отдельным фактом. Просмотр не подтверждает владение метаданными,
не получает source lease и не авторизует применение правки.

`source/previewRead`: `{snapshotId,snapshot,offset,bytes}` и
`source/previewClose`: `{snapshotId}` используют прежние бинарные chunks и
правила close, без metadata context. Preview и project snapshot не могут
читать или освобождать handles друг друга. IDs не переиспользуются в течение
соединения, включая workspace/close. Закрытие workspace уничтожает прежние
handles; новый preview можно открыть по тому же фиксированному корню.
Shutdown и EOF освобождают все данные. Предварительное чтение не требует
установленного анализатора, Git, платформы или сканирования проекта.

Лимиты 16 source views, 32 MiB и 64 entries остаются общими с API 1.17/query.
Отдельного хранилища или квоты для preview нет. `preview_unavailable` означает,
что серверу не удалось установить корень; действуют остальные source view errors.

Для известного источника адаптер сначала вызывает `source/snapshotOpen` с
точным context и native snapshot. Preview используется только без доступного
проекта либо для `unknown_source`; он не подменяет ошибки свежести, IO или
удерживаемый owner активной query-сессии. SDK дополнительно проверяет, что
preview действительно остался disk-only, unclassified и `writable:false`.
Нативная приёмка каждого адаптера остаётся отдельной от headless core/RPC.

Наблюдатели физических источников в адаптере должны проверять схему URI,
включая собственные file watchers, manifest и analyzer-config watchers.
[Проверенная реализация VS Code 1.109.5](https://raw.githubusercontent.com/microsoft/vscode/1.109.5/src/vs/workbench/api/common/extHostFileSystemEventService.ts)
сопоставляет glob с `uri.fsPath`: одинаковый путь другого provider тоже может
доставить событие наблюдателю. Уведомление о read-only представлении не является
изменением его физического исходника и не должно подаваться обратно в ESKA/LSP.


### API 1.19: BSL-черновик с защищённым снимком и проверкой конфликтов

При handshake 1.19 `querySessions` объявляет `bslEditing:true`,
`sourceKinds:["dataCompositionSchema","form","bsl"]`,
`ownerProtection:"protectedViewWithConflicts"`, `bslSave:"checkpointOnly"`.
Клиенты 1.15–1.18 сохраняют `bslEditing:false` и отказ `owner_lock_required`
при попытке открыть или восстановить BSL-сессию. Извлечение литерала T100
не зависит от включения сессий.

BSL использует прежние `query/session*` и durable checkpoint API 1.16.
Открытие сохраняет полный текущий буфер владельца; `source/snapshotOpen`
предоставляет этот же снимок защищённому представлению. Обычные нативные вкладки
могут редактироваться. ESKA не требует токена Native lock и не меняет overrides
IDE. В header BSL `protection.ownerProtection:"protectedViewWithConflicts"`;
у XML это поле null. Прежнее документированное `ownerLock` остаётся null.

Сохранение и autosave BSL-черновика меняют только private checkpoint.
`sessionPrepare` вызывается по явному «Применить». Core повторно проверяет
точную версию и SHA-256 Native-модели, диск, descriptor, логического владельца,
поддержку и application context. Несовпадение приостанавливает применение,
сохраняя исходный снимок и черновик. BSL-план всегда имеет `saveOwner:false`:
адаптер применяет его одной штатной отменяемой текстовой транзакцией,
не сохраняет модуль на диск и подтверждает точный результат через `sessionApplied`.
Новый ввод во время receipt сохраняется отдельно по прежним правилам 1.16.

Закрытие/отмена освобождают только собственный backend source lease.
В `sessionClose` добавлено `releasedSourceRestriction:string|null` — описание
освобождённого ограничения; `releasedOwnerLock` остаётся null. Числовое значение
не предоставляет права записи. Неоконченный intent восстанавливается только
явным продолжением; автоматический replay по-прежнему запрещён.

Восстановление BSL-сессии не требует предварительного раскрытия дерева IDE.
ESKA проверяет объявленную ancestry сохранённого владельца и открывает только
соответствующую ветку модуля. Необъявленные IDs и несовпадающие source/descriptor
по-прежнему отклоняются; восстановленная сессия остаётся paused до явного resume.
[Проверка восстановления после открытия проекта](measurements/t102-bsl-recovery.json).


### API 1.20: неизменяемая языковая проекция BSL

Опциональная capability `queryProjections` объявляется клиенту 1.20 и новее:
`{schemaVersion:1,writesOwner:false,writesDraft:false,sourceKinds:["bsl"],
maxProjections:16,maxSnapshotBytes:8388608,maxChunkBytes:131072,maxPositions:64,
maxEdits:64,sharedQueryBudget:true}`. Старые клиенты не получают capability;
попытки вызвать методы проекции отвечают -32601. XML-контекст не объявляется.

`query/projectionOpen` принимает metadata context, querySessionId, revision
и прежний owner facts. Нужны текущая явно продолженная BSL-сессия, совпадающие
Native версия/SHA-256, диск, descriptor и application context; pending apply
не допускается. Поддержка и пользовательский read-only не запрещают анализ.
ESKA подставляет подтверждённый черновик в полный снимок настоящего модуля через
кодек T100. Файлы, private checkpoint, draft revision и apply intent не меняются.
Сервер не отправляет LSP: транспорт и привязка к своему клиенту принадлежат IDE.

Ответ: metadata envelope и `{projectionId,source,sourceKind:"bsl",revision,
snapshot,bytes,draftSnapshot,draftBytes,readOnly:true,nativeVersion,nativeSnapshot}`.
`source` — проверенный относительный путь владельца; `snapshot` — SHA-256 полного
проектируемого текста UTF-8, `draftSnapshot` — SHA-256 декодированного SDBL с LF.
`projectionId` обозначает неизменяемый снимок, не право записи владельца.

Последующие методы получают metadata context и projectionId; кроме Close также
обязателен snapshot. Их ответ не содержит metadata envelope. После смены generation
чтение и cleanup разрешены прежнему session/member; другой member/process не может
использовать handle. Workspace close, EOF и shutdown освобождают проекции.

- `query/projectionRead`: `{projectionId,snapshot,offset,bytes}` возвращает
  `{projectionId,snapshot,offset,nextOffset,eof,data}`. data — canonical base64;
  offset/bytes измеряются в байтах, допускается граница внутри UTF-8 scalar.
- `query/projectionMap`: `{projectionId,snapshot,direction,positions,affinity?}`.
  direction: `draftToOwner`, `ownerToDraft` или `ownerTarget`; positions — UTF-16
  `{line,character}`. affinity `before|after` разрешён только draftToOwner,
  по умолчанию after. Ответ `{positions:[null|{target:"draft"|"owner",position}]}`.
  ownerTarget переводит позиции внутри запроса в черновик, а позиции неизменённого
  кода — в исходный Native-буфер с учётом сдвига литерала. Quotes, `|`, отступы
  и внутренности escapes не округляются к соседнему символу.
- `query/projectionEdits`: `{projectionId,snapshot,edits:[{range:{start,end},newText}]}`
  принимает UTF-16 правки языка в проектируемом BSL. Весь набор проверяется:
  границы должны принадлежать запросу, пересечения и выход за литерал запрещены.
  После повторного декодирования ESKA выдаёт план только для SDBL:
  `{noOp,inputSnapshot,outputSnapshot,replacements:[{range,rangeUtf16,text}]}`.
  Это не изменение сессии или Native-модуля. IDE проверяет актуальную ревизию
  черновика и применяет результат через его Undo/Redo.
- `query/projectionClose`: `{projectionId}` → `{released:bool}`; повторный close
  допустим. Проекция не удерживает source lease и не мешает завершению сессии.

Всего до 16 проекций, до 64 позиций/правок за запрос; тело ограничено 8 MiB.
Оригинал, проекция, декодированный текст и карта учитываются в общей квоте
query 32 MiB/64 entries. Карта ограничена 65536 spans. Сохранённая проекция
доступна после закрытия source session, не сохраняя её временное ограничение.
Reasons: `unknown_projection`, `projection_limit`, `projection_snapshot_mismatch`;
действуют прежние stale/context/codec/resource errors. JSON не зависит от locale.


### API 1.21: конструктор и позиции исходных комментариев

Capability `queryConstructor` объявляется с 1.21: schemaVersion=1,
writesOwner=false, writesDraft=true, explicitFinish=true, exclusiveDraft=true,
lossless=true, nativeDraftPlan=true, sourceCommentPositions=true, sharedQueryBudget=true,
maxExpressionBytes=16384, maxModelBytes=33554432. Clauses:
`fields,sources,joins,conditions,group,order`. Старые клиенты получают -32601.
Хранилище checkpoint остаётся прежнего формата; незавершённая рабочая модель
конструктора хранится в памяти и не применяется после перезапуска.

Все методы получают metadata context и querySessionId. Open/Action/Finish
требуют текущего generation и свежего CAS checkpoint. Model/Plan/Cancel/sessionComments
разрешены прежнему session/member после metadata refresh, без metadata envelope.
Чужой member или process не может использовать source handle.

- `query/constructorOpen`: `{querySessionId,revision}`. Нужна Ready-сессия без
  ожидаемого source Apply и другого конструктора. Возвращает session header и
  `constructor`. В header добавлен `constructorActive`; `draftWritable=false`
  на время конструктора. Не изменяет canonical draft, source lease и файл.
- `query/constructorModel`: `{querySessionId,constructorId}` читает текущую модель.
- `query/constructorAction`: `{querySessionId,constructorId,constructorRevision,
  action}` меняет только рабочую модель. Action: `append` с statement, clause,
  text, необязательным join (`inner|left|right|full`); `replace` с selection/text;
  `remove` с selection и comments (`keep|discard`); `move` с selection/to.
  Selection: `{statement,clause,item}` — индексы строго этой ревизии.
- `query/constructorPlan`: `{querySessionId,constructorId,constructorRevision,
  nativeSnapshot}` выдаёт `draftPlan` без изменения canonical draft, checkpoint
  или владения. Рекомендуемый порядок IDE: Plan, проверка Native версии и одна
  правка SDBL, затем Finish с исходным nativeSnapshot и фактическим resultSnapshot.
  При сбое между Native-правкой и подтверждением текст остаётся в модели IDE;
  исходник XML/BSL по-прежнему не меняется.
- `query/constructorFinish`: `{querySessionId,constructorId,constructorRevision,
  nativeSnapshot,resultSnapshot?}` сверяет SHA-256 исходного canonical SDBL, публикует изменённый
  draft через общий CAS и освобождает только его владение. Ответ содержит
  session header и `draftPlan` в том же byte/UTF-16/hash формате, что projectionEdits.
  IDE проверяет Native версию и применяет его одной отменяемой правкой к той же
  вкладке SDBL. Finish никогда не выдаёт source apply token и не сохраняет модуль/XML.
  При отказе Native-правки durable результат остаётся в private checkpoint;
  адаптер обязан сохранить также более новый Native ввод и показать конфликт.
- `query/constructorCancel`: `{querySessionId,constructorId}` отбрасывает рабочую
  модель, сохраняя canonical draft и его ревизию. Не зависит от последней
  constructorRevision, поэтому может завершить прерванный Action. Повторный cancel
  после освобождения возвращает released=false; чужой активный token отклоняется.

Модель: `{id,revision,snapshot,text,bytes,writable,statements}`. В statement есть
range, unavailable, preview, clauses и append options с причинами недоступности.
Clause содержит index/kind/header/items. Item содержит index, byte range, preview,
expression (null при превышении 16 KiB), comments count, removable, movable и
unavailable reasons. Неизвестные операторы пакета, UNION/INTO/TOTALS/HAVING,
расширения СКД и неоднозначные OR/BETWEEN/CASE остаются opaque. Вложенные выражения
не регенерируются. Переносы JOIN не разрешаются без проверки зависимостей.

ESKA хранит исходные ranges и комментарии; перемещение строки переносит её
комментарии. Удаление узла с комментариями требует keep/discard. No-op сохраняет
все байты. Concurrent recoveryCommit с другим draft, recoveryRemove, resume,
source Apply и языковая проекция под владением конструктора отклоняются.
Неизменённый checkpoint допустим. Query/session close и EOF освобождают владение.
Лексер ограничен 65536 токенами и 256 операторами; более 128 разделов либо
2048 верхнеуровневых разделителей делают оператор opaque с reason=model_limit.
Разделение пакета выполняется одним проходом, без повторного сканирования
всех токенов для каждой строки. Квота рабочей модели входит в прежние 32 MiB query state; новых индексов нет.

`query/sessionComments`: `{querySessionId,revision,positions:[{line,character}]}`
возвращает revision, snapshot canonical draft, needsDecision, lineStartsOnly,
исходные comments (`comment,text,originalQueryOffset,originalQueryPosition`) и
positions (`position,queryOffset|null`). До 64 UTF-16 позиций; surrogate/CRLF
interiors не округляются. Для BSL допустимы только начала строк после первой.
Это чтение исходных якорей общим кодеком, без source plan. Полученный byte offset
передаётся в прежний sessionPrepare.comments; null там означает явное удаление,
поэтому адаптер не должен подставлять недопустимую позицию вместо решения.

Reasons: constructor_busy, unknown_constructor, constructor_limit,
constructor_invalid_syntax, constructor_unsupported, constructor_invalid_selection,
constructor_invalid_expression, constructor_required_item, constructor_source_required,
constructor_unsafe_move, constructor_comments_require_decision, stale_constructor;
действуют прежние stale/recovery/read-only errors. UI и URI IDE в ядро не входят.


### API 1.22: выбор метаданных и структурированные операции конструктора

В `queryConstructor` для клиента 1.22 добавлены `visualExpressions=true` и
`metadataCatalog=true`. Методы/действия 1.22 не доступны клиентам с меньшим minor;
прежние операции 1.21 сохраняются. Формат checkpoint не меняется.

`query/constructorCatalog` получает metadata context, `querySessionId`,
`constructorId`, `constructorRevision` и `query`:
`{search?:string,offset?:number,limit?:number,table?:string,qualifier?:string}`.
Он проверяет текущий generation, владельца конструктора и fingerprint его
выбранного приложения. Ответ — прежний session header и `catalog`:
`{tables,fields,nextOffset,found,contextFingerprint,limitedContext}`.
Действие не меняет конструктор, canonical draft, checkpoint и исходник; SDK
не перечитывает части черновика ради выбора метаданных.

Без `table` перечисляются физические таблицы объявленных источников приложения;
`search` ищет по русскому и английскому имени без учёта регистра. Порядок
определяется полным английским именем. `offset` по умолчанию 0, `limit` — 200,
максимум 500; `nextOffset=null` означает конец. Элемент таблицы:
`{name,category,russian,english,alias}`. Category — существующее имя типа
метаданных (`Catalog`, `Document`, `InformationRegister` и т. п.), а не подпись UI.
Одинаковые таблицы базы и включённых расширений объединяются; чужие приложения
не участвуют. Search ограничен 1024 байтами, table — 4096.

С `table` ESKA читает только соответствующий дескриптор каждого входящего
источника. `fields` содержит `{russian,english}`; `tables` — доступные табличные
части. Имена полей при `qualifier` уже квалифицированы backend. Qualifier — имя
псевдонима либо полное имя источника из идентификаторов через точку, до 1024 байт.
`found=false` отличается от ошибки чтения дескриптора. Список не объявляет
полноту всех платформенных и виртуальных колонок: возвращаются поддержанные
стандартные поля и реальные реквизиты, измерения и ресурсы. Корреспонденция
бухгалтерского регистра учитывает раздельные счета и небалансовые поля Dr/Cr.
Обычной таблице регистра нумерованные субконто не приписываются.

В statement модели добавлены `russian:bool` и `sources:[{table,alias}]`. При
неявном псевдониме alias содержит полное имя источника. В item добавлено
`visual:null|Expression`: непризнанная либо слишком длинная строка остаётся
доступной в прежнем expression/preview без выдуманного структурированного разбора.

Новые действия `query/constructorAction`:

- `compose`: `{kind:"compose",statement,selection:null|{statement,clause,item},
  expression}`. Null добавляет элемент, selection меняет элемент того же типа.
  Expression — tagged object с `kind`: `source` (`table,alias`), `field`
  (`expression,alias,aggregate:null|sum|count|minimum|maximum|average`), `join`
  (`table,alias,join,left,comparison,right`), `condition` (`left,comparison,right`),
  `group` (`expression`) или `order` (`expression,descending:bool`). Comparison:
  `equal,notEqual,less,lessOrEqual,greater,greaterOrEqual,like,in,inHierarchy`.
  Ключевые слова, AS/КАК, агрегатные вызовы и направление строит ESKA в языке
  оператора. Пустой alias нового источника получает уникальное имя. Простое имя
  правого поля нового JOIN квалифицируется выбранным alias. Переименование
  источника меняет доказуемые ссылки; строки, комментарии и пути метаданных
  сохраняются. Смена типа JOIN меняет только нужные слова заголовка.
- `appendFields`: `{kind:"appendFields",statement,fields:[string]}` — до 512
  выражений. Добавление всего списка атомарно, выбранные неагрегатные выражения
  повторно не добавляются. При отказе любой строки прежние текст и revision
  остаются неизменными.

Действуют прежние лимиты выражения/модели, CAS, read-only и правила комментариев.
Ни каталог, ни визуальные операции не выполняют запрос в ИБ. Виртуальные
параметры, UNION/INTO/TOTALS/HAVING, вложенные источники и расширения СКД не
объявлены новым визуальным подмножеством; неизвестный текст сохраняется.

### API 1.23: виртуальные таблицы и составные запросы

`queryConstructor.virtualTables=true` разрешает структурированные вызовы
виртуальных таблиц. В `query/constructorCatalog.query` добавлен
`virtualTables?:bool` (по умолчанию false). При раскрытии регистра возвращаются
доступные виртуальные таблицы с `virtualTable` — стабильным английским enum.
При выборе такой таблицы `catalog.parameters` содержит упорядоченные
`{key:string,required:bool,choices?:[{russian,english}]}`. Подписи параметров локализуются адаптером;
позиционные значения, включая пропущенные необязательные позиции, хранит ESKA.

`compose.expression` получает варианты `virtualSource` (`table,alias,parameters`)
и `virtualJoin` (те же поля плюс `join,left,comparison,right`). `parameters` —
массив выражений, не готовый фрагмент вызова. Backend проверяет границы каждого
значения и формирует скобки/разделители. `sources[].table` содержит путь таблицы
без аргументов, `sources[].parameters` — отдельный массив для существующего вызова.
Несовместимые запросы от клиента ниже 1.23 получают `-32601`; его DTO не содержит
новых вариантов `visual`.

Доступность и поля определяются метаданными регистра: периодичностью, видом,
корреспонденцией, длиной уточнения периода и назначенным планом счетов.
Для регистров расчёта `ScheduleData` читает ресурсы назначенного графика,
а `Base<register>` — явно связанные базовые планы видов расчёта и их регистры.
Каталог связанного `query.source` сам распознаёт виртуальный источник; вызов
без необязательных скобок сохраняется побайтно при неизменённой форме.

`nestedQueries=true` и `unions=true` добавляют `constructor.scopes`:
`[{index,range,parent:number|null,branches:[statementIndex],result:statementIndex,unions:[{all:bool}]}]`.
У каждого statement есть `scope`, порядковый `branch`, `nested` и `result`.
Последний флаг указывает ветку с общим порядком/итогами; внутри вложенной
области эти секции не создаются. Все индексы привязаны к ревизии конструктора.
Лимиты: `maxScopes=256`, `maxNesting=32`; общий предел ветвей/операторов — 256.

Выражения `subquerySource:{alias}` и `subqueryJoin:{alias,join,left,comparison,right}`
создают вложенный SELECT средствами ESKA. Существующий внутренний текст не
передаётся через форму: изменение псевдонима и связи сохраняет его диапазон.
Для каталога конкретного источника можно передать `query.source` — selection
`{statement,clause,item}`. ESKA возвращает поля его физической/виртуальной таблицы
либо проекции первой ветки вложенного запроса, разворачивая поддержанные `*`.

Действия `addUnion:{statement,all}`, `setUnion:{statement,all}` и
`removeUnion:{statement,comments?}` создают копию выбранной ветки, меняют вид
объединения и удаляют ветку. Общий хвост порядка/итогов сохраняется; при
удалении первой ветки сохраняются доказанные общие псевдонимы результата.
Добавление, перестановка и удаление поля выполняются во всех ветках атомарно;
для ещё не заданного значения ESKA добавляет `NULL`. Комментарии требуют прежнего
явного решения. `issues:[{code:"union_field_count",scope}]` описывает известное
несовпадение количества полей; изменённый запрос с такой ошибкой не публикуется.
При переименовании доказанного выходного поля ESKA обновляет ссылки родителей
и общего хвоста, исключая строки, параметры и затенённые области.

`totals=true` расширяет `clauses` до `fields,sources,joins,conditions,group,order,totals,totalGroups`.
`totalField:{expression,alias,aggregate}` задаёт итоговое выражение, `overall`
добавляет общие итоги, `totalGroup:{expression,alias,hierarchy,period}` — группировку.
`hierarchy` принимает `none|hierarchy|onlyHierarchy`; `period:null|{unit,begin,end}`
задаёт заполнение периодов, где границы — выражения либо null, а единица —
`second|minute|hour|day|week|tenDays|month|quarter|halfYear|year`.
ESKA создаёт целую секцию `TOTALS … BY`, допускает пустые `totals.items` для
автоматических итогов и удаляет секцию при удалении последней группировки.
`overall` всегда первый; его повторное добавление не меняет текст.

`query.scope` в каталоге возвращает выходные колонки области; он взаимоисключающий
с `query.source`. `statements[].columns` содержит доказанные выходные имена
первой ветки. `sources[]` дополнительно содержит `clause,item,derived,nestedScope`;
у элемента есть `children:[scopeIndex]`, `moveUp,moveDown` с разрешениями для
каждой стрелки. Это навигационные данные ESKA, а не результат разбора в IDE.
`constructor.focus:number|null` возвращает оставшуюся ветку исходной области
после структурной операции: удаление ветки не должно открывать вложенный
источник, занявший её прежний плоский индекс.
Клиенты ниже 1.23 видят составные запросы и итоги как точный непрозрачный текст;
их операции над такими областями отклоняются до изменения состояния.

### API 1.24: обратная навигация по исходнику

`initialize` с minor ≥ 24 объявляет `capabilities.sourceNavigation: true`.
`metadata/revealSource` принимает обычные `sessionId,projectId,generation` и
`path: WirePath` относительно source выбранного проекта. Абсолютные пути,
выход за source и внешние symlink targets отклоняются. Клиенты ниже 1.24
получают `-32601`; прежний `metadata/reveal` по `objectId` не меняется.

ESKA разрешает владельца через общий Designer resolver, проверяет объявления
предков и точное соответствие `metadata/source`. Метод возвращает
`{source: SourceLocation|null, nodes: MetadataNode[], sessionId, projectId,
generation, eventSequence}`. `nodes` — включительный путь от корня до объекта
или модуля с обычными структурированными labels и parent links. У неизвестного,
необъявленного или удалённого исходника `source:null, nodes:[]`; ошибки чтения
объявленных descriptors сохраняются как ошибки, а не маскируются пустым ответом.

Не запускается поисковый индекс, не читаются XML соседей и не передаются списки
всех соседних объектов. Разрешение владельца не зависит от раскрытых ветвей IDE.
Адаптер сохраняет только этот путь, отображает форму/модуль в принятом для IDE
виде и при раскрытии получает полный список через `metadata/children`.
Метод не пишет исходники, manifest или Git. Обычные проверки поколения,
`requiresRefresh` и `requiresReopen` продолжают действовать.
