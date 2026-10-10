# Структура текущего BSL-документа

`eska-analyzer` обслуживает `textDocument/documentSymbol` через общий
`AnalysisSession::module_document_symbols()`. Процедуры, функции, переменные
модуля и области берутся из item/region trees закреплённого ядра. Второго parser,
индекса, XML-входа и дополнительной копии текста нет. Внутренний результат
сохраняет typed details ядра, включая экспортность, directives и параметры.
Транспорт публикует имена, виды и диапазоны; сигнатуры пока не отображаются.
Дерево метаданных остаётся отдельным `eska/metadata/*` API.

## Запрос и согласование

После ESKA handshake `0.1` и initialized клиент передаёт URI объявленного BSL
и необязательный `eska.snapshot`. Snapshot проверяется до IO; без него запрос
использует текущую упорядоченную сессию. Pending caller читается лениво,
соседние YAML/BSL не загружаются. Settings фиксируются при initialize:
повреждённый или неподдержанный `bsl-analyzer.toml` даёт явный отказ language query;
отключённый ParseError не отключает структуру.

При корректных settings объявляется `documentSymbolProvider=true`.
Явный `textDocument.documentSymbol.hierarchicalDocumentSymbolSupport=true`
выбирает `DocumentSymbol[]`: name, kind, range, selectionRange и children.
Остальные клиенты получают `SymbolInformation[]`: name, kind, location
и containerName непосредственного родителя. Плоский список идёт в preorder;
его диапазоны не используются для восстановления иерархии. Эти формы предусмотрены
[LSP document symbols](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/language/documentSymbol.md).
Capability `experimental.eska.documentSymbols` сообщает scope `current-bsl-module`,
profile `structure` и согласованную форму `hierarchical` или `flat`.

Процедура и функция имеют стандартный kind 12, переменная — 13, область — 3,
как в закреплённом BSL-сервере. Все виды входят в исходный набор LSP.
`eska.kind` сохраняет отдельные canonical значения procedure/function/variable/region.
Символы внутри областей сохраняют порядок; области внутри тела метода не
подменяют структуру модуля. workDoneToken/partialResultToken принимаются,
progress и частичные ответы не отправляются.

## Текущий снимок и ошибки

Каждый узел содержит `eska`: snapshot, source/hash, profile, byte endpoints
объявления и выделяемого имени, coverage и resources. Диапазоны UTF-16 получаются
одним проходом по заимствованному текущему тексту. BOM, non-BMP и CRLF учитываются;
selectionRange находится внутри range, дочерний узел — внутри родителя.
Неизвестный модуль, недопустимый URI и stale snapshot отклоняются до чтения.
Пустое имя или несогласованные диапазоны ядра дают document_symbol_inconsistent,
`-32603`, вместо правдоподобных координат с округлением.

Недоступный, двоичный или отказанный по квоте caller даёт module_unavailable.
Готовый пустой модуль возвращает стандартный `[]` без дополнительных полей.
Неполнота соседних inputs не запрещает структуру текущего модуля: этот запрос
не требует разрешения метаданных. Coverage остаётся явным на непустых результатах;
semantic_complete всегда false. Восстановленные после синтаксической ошибки
объявления не означают успешную диагностику или полноту семантики.
Editor bytes имеют приоритет; close читает фактический сохранённый BSL.

Пять languageMethods и шесть cancellableMethods включают documentSymbol
и последующий [hover символа](analyzer-symbol-hover.md).
Общий request control сохраняет lifetime ID, input thread и одну очередь/worker.
Клиентская отмена — `-32800`; deadline — `-32000` без retriggerRequest.
Нулевой timeout останавливает запрос до lazy IO; обычный — 300 секунд.
Отмена кооперативная: предел задержки не установлен. Проверка и транспортное
построение дерева используют итеративный обход, без собственной рекурсии.

## Проверки

82 unit/file, 80 analyzer process и 11 checker process tests прошли: всего 173;
minimal — 66. Проверены текущие имена и typed details, вложенность, Unicode/EOL,
saved/editor/close, пустой и недоступный caller, настройка клиента, frozen settings,
snapshot/URI guards, таймер и отмена с числовым/Unicode ID и последующим повтором.
Машинные ответы RU/EN совпадают; новых human strings и dependencies нет.
Все 19 tool gates и 15 root gates прошли: fmt/check, strict Clippy/Rustdoc,
тесты feature profiles и release tools. Root — 894 passed/14 прежних ignored.

Проверки реальных модулей и измерения записаны в
[обезличенном результате](analyzer-document-symbols-results.json).
24 сценария проверили 144 pulls и 768 узлов: два реальных модуля каждого C1/C2/C3,
flat/hierarchical и RU/EN. Saved/shifted/augmented/empty/closed, stale refusal,
hash/byte/UTF-16, равенство форм и locale traces подтверждены независимо от core.
Полные составы используются с частичными копиями payload и editor overlays;
12 выбранных исходных файлов и байты копий сохранены, временные проекты удалены.
HWM — 40 740–196 944 KiB, измеренные queries — 0.000169–0.005586 секунды.
Это ограниченный замер, не бюджет heap/RSS и не полный проход больших конфигураций.
Это stdio-проверки текущих снимков, а не запуск
в редакторе или платформе 1С. Полная семантика, общий heap/RSS budget,
остальные IDE edits и Explorer остаются условиями незавершённой EF-18.
