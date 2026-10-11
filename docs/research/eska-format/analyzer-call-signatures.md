# Подсказки параметров текущего BSL-вызова

`eska-analyzer` обслуживает стандартный `textDocument/signatureHelp` через
`AnalysisSession::module_signature_help()`. Общее ядро выполняет native call
binding, выбирает перегрузки и активный аргумент. Core переносит готовые
сигнатуры, параметры и документацию в `CallSignatures`; второго parser,
семантического разрешения или локального ранжирования кандидатов нет.
Запрос ядра остаётся read-only. Сервер отдельно выполняет
[адресную подготовку](analyzer-targeted-signatures.md) own context и прямого
common/manager target для активного member call. Соседние inputs не читаются.

Сигнатуры — каноническое BSL-представление закреплённого ядра. Ключевые слова
и имена типов в нём русские независимо от locale CLI; это код, а не сообщения
интерфейса. Purpose и parameter documentation получены нативным разбором
комментариев или платформенной моделью. Авторский текст не переводится.

## Протокол и возможности клиента

После handshake `0.1` и initialized запрос принимает textDocument URI,
position в UTF-16, необязательные context, workDoneToken и `eska.snapshot`.
Типизированный decoding выполняется до общей проверки snapshot; stale/unknown
session и URI проверяются до lazy IO. Pending caller читается по запросу,
CancelledSaved можно повторить, editor input не заменяется сохранённым файлом.
Settings фиксируются при initialize. Повреждённые settings запрещают языковой
запрос и публикацию provider; отключённый ParseError его не отключает.

При корректных settings `signatureHelpProvider` сообщает triggerCharacters
`(` и `,`. Только при `textDocument.signatureHelp.contextSupport=true`
добавляется retriggerCharacters `)`. workDoneProgress=false; progress не
отправляется. partialResultToken не входит в signatureHelp params и отклоняется.
Контракт определяется [LSP signature help](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/language/signatureHelp.md).

Context проверяет triggerKind 1–3, обязательный isRetrigger и один Unicode
символ triggerCharacter при kind 2. activeSignatureHelp принимается как
необязательный JSON-объект с непрозрачным содержимым. Прежняя карточка клиента
не подменяет текущие inputs, порядок кандидатов или выбор параметра: ядро
вычисляет результат заново. Размер входа ограничен общим JSON-RPC frame limit.

`textDocument.signatureHelp.signatureInformation` согласует четыре независимых
возможности. Первый поддержанный documentationFormat выбирает plaintext или
markdown; по умолчанию используется plaintext. parameterInformation.labelOffsetSupport
выбирает числовые UTF-16 диапазоны label либо строковые labels. activeParameterSupport
разрешает отдельное поле активного параметра выбранной SignatureInformation.
noActiveParameterSupport разрешает null при отсутствии активного параметра
в SignatureHelp и, если согласовано, в выбранной SignatureInformation.
Для прежних клиентов отсутствие параметра передаётся пропуском поля и следует
стандартному LSP default; нативный nullable выбор дополнительно сохранён в `eska`.

`experimental.eska.signatureHelp` сообщает scope `current-bsl-call`, выбранные
documentationFormat и parameterLabel (`string`/`utf16-offsets`), incompleteResults
`error`, clientHints `recompute` и inputPreparation `direct-member-targets`.
Порядок signatures и activeSignature сохранены
из call binding. В сигнатуре метода, не выбранной ядром, per-item activeParameter
не появляется. Сигнатура без параметров сохраняется с parameters=[];
пустой список signatures не выдаётся как успешная подсказка.

## Диапазоны, текущий ввод и ограничения

Core проверяет порядок native parameter labels в сгенерированной сигнатуре и
создаёт byte_range относительно этой строки. Это не диапазон исходного BSL.
Параметр с именем метода выделяется внутри скобок, а не в заголовке.
Transport одним проходом переводит диапазоны в UTF-16; non-BMP символ в значении
по умолчанию не сдвигает последующий параметр на неверное число кодовых единиц.
Несогласованные labels или active indices дают signature_help_inconsistent,
`-32603`; правдоподобные диапазоны не подбираются. Новый вариант экспериментального
`QueryError` — InconsistentSignatureHelp; API ещё не стабилизирован.

Документация использует общий с hover компонент markup. Plaintext сохраняет
текст; markdown экранирует punctuation, поэтому содержимое комментариев не
превращается в ссылки или HTML. BSL label остаётся обычной строкой протокола.
Core не содержит служебных UI-заголовков или локализации.

`eska` содержит snapshot, caller source/hash, profile `call-signature`,
position_byte, nullable active_signature/active_parameter, coverage и resources.
Позиция относится к текущему месту вызова. Если целевой метод находится в другом
ready общем модуле, его изменение обновляет подсказку, сохраняя hash caller.
Переименованный или ошибочный текущий YAML убирает прежнюю разрешённую сигнатуру;
close восстанавливает фактические saved bytes.

Отсутствие результата при неполных inputs даёт signature_help_incomplete, `-32000`;
при готовых inputs — стандартный null. Недоступный/binary/quota-denied caller
даёт module_unavailable. Непустые результаты сохраняют явные coverage flags,
semantic_complete всегда false. Это подключение native API, не обещание полного
анализа всех типов, перегрузок, параметров форм или платформенных контекстов.

Преобразование LSP позиции поддерживает CRLF, LF и одиночный CR. Закреплённый
native lexer завершает `//`-комментарий только на LF: в файле с одними CR
такой комментарий может скрыть последующий вызов. Bytes не нормализуются и
не исправляются скрыто. Проверка CR в process tests изолирует преобразование
позиций на BSL без line comments; устранение ограничения lexer сюда не входит.

Девять languageMethods и десять cancellableMethods включают signatureHelp,
[references](analyzer-references.md) и [локальные prepareRename/rename](analyzer-local-rename.md).
Общий request control сохраняет отмену `-32800`, deadline `-32000` без
retriggerRequest, нулевой timeout до IO и обычный лимит 300 секунд.
Отмена кооперативная; второй worker, host, индекс или dependency не добавлены.

## Исходная проверка native signatureHelp

Приведённые ниже результаты относятся к первому подключению native query.
Актуальные cold member inputs и проверки описаны в
[адресной подготовке](analyzer-targeted-signatures.md).

95 unit/file, 97 analyzer process и 11 checker process tests прошли: всего 203;
minimal — 73. Проверены nested calls, comma внутри string, текущие parameter/docs,
связь двух модулей и rename/invalid YAML, BOM/non-BMP, UTF-16 labels, оба вида
markup, nullable selection, игнорирование прежней карточки, guards/settings,
incomplete/null, native перегрузки конструктора и метод typed платформенного
объекта, отмена с numeric/Unicode ID и продолжение той же сессии.
Все 19 tool и 15 root gates прошли; root — 894 passed/14 прежних ignored,
minimal — 165/3, project-cli — 182/3, build-cli — 201/3.

[Обезличенные измерения](analyzer-call-signatures-results.json) фиксируют
24 сценария C1/C2/C3: два реальных модуля, plaintext/string либо markdown/offsets,
RU/EN. Вызов реального метода добавлен только в editor overlay собственного
проекта; сохранённые реальные декларации не изменены. Всего 240 pulls:
144 сигнатуры, 72 incomplete и 24 stale refusals. Synthetic overlay дополнительно
проверяет nested/строковые аргументы и изменение параметра/документации.
Пустой черновик и close проверяют current hash и освобождение source reservation.
Locale traces сравниваются целиком после нормализации opaque session IDs.
12 выбранных оригиналов и bytes копий сохранены; уникальные проекты удалены.

Normal runtime graph `analyzer-cli`, x86_64-unknown-linux-gnu: 206 имён/214 версий;
lsp-server, lsp-types, Tokio, reqwest, gix и gix-transport отсутствуют.
HWM — 66 800–217 992 KiB, измеренные queries — 0.000113–0.060340 секунды.
Это ограниченные измерения частичных копий, не heap/RSS budget
или полный проход больших конфигураций. Проверки выполнялись через stdio;
редактор, watcher ОС и платформа 1С здесь не запускались. Полная семантика,
общий memory budget, остальные IDE edits и Explorer остаются в незавершённой EF-18.
