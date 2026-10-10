# Карточка текущего символа BSL

`eska-analyzer` обслуживает стандартный `textDocument/hover` через
`AnalysisSession::module_symbol_hover()`. Общая сессия запрашивает структурированную
позиционную карточку `ide::symbol_info` закреплённого ядра: имя, вид, BSL-объявление
метода, purpose из комментария и возвращаемый тип метода. Второго parser,
семантической модели или project host нет; query не читает XML и соседние inputs.

Готовый Markdown ядра здесь не используется: его служебные заголовки содержат
русские строки независимо от locale. Core возвращает `SymbolHover` и `HoverKind`,
а IDE presentation переводит заголовки и оформляет typed данные. Неэкспортируемые
native XML locations и graph ids в карточку не попадают. Каноническая сигнатура
метода остаётся BSL-кодом; purpose сохраняет авторский язык после разбора
комментария ядром. Текст комментария не переводится как сообщение интерфейса.

## Запрос и представление

После handshake `0.1` и initialized запрос содержит URI объявленного BSL,
position в UTF-16 и необязательный `eska.snapshot`. Snapshot/URI проверяются
до lazy IO. Pending caller читается по запросу, saved CancelledSaved допускает
повтор, editor input сохраняет приоритет. Settings фиксируются при initialize.
Повреждённая конфигурация анализа запрещает hover и не публикует provider;
отключённый ParseError не отключает карточку.

При корректных settings объявляется `hoverProvider=true`. Первый поддержанный
формат в `textDocument.hover.contentFormat` выбирает MarkupContent plaintext
или markdown; отсутствующий, пустой или неподдержанный список даёт plaintext.
Ответ содержит contents и range. Эти поля определены в
[LSP hover](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/language/hover.md).
workDoneToken принимается; progress не отправляется. partialResultToken
не является параметром hover и отклоняется как rpc_invalid_params.

`experimental.eska.hover` сообщает scope `resolved-bsl-symbol`, согласованный
contentFormat и incompleteResults `error`. Native вид преобразуется в закрытый
машинный enum: method, module_variable, parameter, local_variable,
platform_function, module, metadata_object, manager_module, field.
Заголовки и названия разделов есть в RU/EN; имена JSON и значения enum не переводятся.

Plaintext сохраняет текст карточки. В Markdown имена, типы и purpose экранируются
как буквальный текст, поэтому authored punctuation не превращается в ссылки,
изображения или HTML. BSL-объявление заключено в code fence длиннее любой
последовательности backticks в данных. Это локальное оформление typed полей,
без разбора или исправления готовой Markdown-строки ядра.

## Текущий снимок и пределы

`eska` содержит snapshot, source/hash caller, profile `symbol`, position_byte,
kind, start_byte/end_byte текущего токена, coverage и resources. Диапазон относится
к имени под курсором, включая оба его края; данные другого ready модуля могут
попасть в карточку, но его hash и расположение не подменяют координаты caller.
Изменение target BSL обновляет возвращаемый тип; переименование текущего YAML
убирает прежнюю разрешённую карточку. Close восстанавливает фактические saved bytes.

Native позиционный API принимает UTF-16 столбцы по собственному line index.
Core преобразует токен через этот же index после проверки границ UTF-8;
transport отдельно преобразует LSP position и текущий byte span. Это сохраняет
BOM, non-BMP, CRLF/LF и одиночный CR без смешивания разных правил строк.

Карточка может быть показана при неполноте соседних inputs с явными coverage flags.
Если символ не разрешён и входы неполны, возвращается hover_incomplete, `-32000`;
при готовых входах отсутствие карточки даёт стандартный null. Недоступный,
двоичный или отказанный по квоте caller даёт module_unavailable. Invalid position,
stale snapshot, неизвестный kind ядра и несогласованный span имеют отдельные ошибки.
Новых вариантов экспериментального `QueryError` два: InconsistentHover
и UnsupportedHoverSymbol; compatibility layer для этого API не создаётся.

Scope ограничен разрешёнными символами позиционного API. Локальные переменные,
созданные только присваиванием, этим API пока не разрешаются; явно объявленные
`Перем`, параметры и переменные модуля проверены. Объявление публикуется только
для методов: labels остальных видов содержат native human prose.
Возвращаемый тип сейчас доступен только для метода; тип локальной переменной,
подсказки keyword/expression и полная platform documentation ещё не подключены.
semantic_complete всегда false; null не означает полную семантическую проверку.

Пять languageMethods и шесть cancellableMethods включают hover. Используется
общий request control: отмена `-32800`, deadline `-32000` без retriggerRequest,
нулевой timeout до IO, обычный 300 секунд. Отмена остаётся кооперативной.

## Проверки

89 unit/file, 87 analyzer process и 11 checker process tests прошли: всего 187;
minimal — 70. Проверены docs/type из текущего BSL, оба края имени, разные EOL,
связь двух модулей, изменение YAML, сохранение bytes, locale machine parity,
negotiation/escaping, invalid settings, incomplete/null и stop controls.
Отмена с числовым/Unicode ID и дальнейшим повтором включена в общий process test.
Все 19 tool gates и 15 root gates прошли; root — 894 passed/14 прежних ignored.
12 новых ключей локализации присутствуют в обоих файлах main.ftl, всего 983 ключа;
новых зависимостей нет.

[Обезличенные измерения](analyzer-symbol-hover-results.json) фиксируют 24 сценария:
два реальных модуля каждого C1/C2/C3, plaintext/markdown и RU/EN. Проверены
192 hover pulls: 120 current cards, 48 явных incomplete и 24 stale refusals.
Saved/shifted, добавленный метод и смена Number→String, comment/empty и close
проверяют независимые hash/byte/UTF-16 координаты. Human contents исключаются
из locale comparison; остальные поля сравниваются целиком после нормализации session id.
Соседние YAML/BSL не загружены; 12 выбранных исходников и байты копий сохранены,
уникальные временные проекты удалены. HWM — 67 196–219 052 KiB, queries —
0.000125–0.059170 секунды. Это ограниченный stdio-замер с полными составами
и частичными payload copies, не общий heap/RSS budget, полный анализ конфигураций,
проверка редактора или runtime платформы 1С. EF-18 остаётся IN_PROGRESS.
