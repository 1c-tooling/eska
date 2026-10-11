# Автодополнение из текущего снимка YAML и BSL

`eska-analyzer` обслуживает стандартный `textDocument/completion` тем же
`AnalysisSession::module_completions`, который доступен библиотечному потребителю.
При корректном `bsl-analyzer.toml` initialize объявляет `completionProvider`:
`resolveProvider=false`, `triggerCharacters=["."]`, `workDoneProgress=false`.
Отключение `ParseError` отключает syntax pull, но сохраняет автодополнение.
Неверные settings дают явную ошибку языкового запроса и не отключают дерево.
Настройки фиксируются при initialize; для перечитывания нужен новый процесс.

## Запрос и ответ

После согласования ESKA protocol `0.1` и `initialized` клиент передаёт:

```json
{
  "textDocument": {"uri": "file:///path/to/source/CommonModules/Tools/Ext/Module.bsl"},
  "position": {"line": 3, "character": 8},
  "eska": {"snapshot": {"session_id": "<current-session>", "revision": "<current-revision>"}}
}
```

ESKA snapshot необязателен для стандартного языкового запроса; при наличии
он проверяется до чтения. URI должен адресовать объявленный локальный BSL-модуль.
Координаты — нулевые UTF-16 line/character текущих байтов. По
[контракту Position](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/position.md)
столбец после конца строки ограничивается её концом. Отсутствующая строка,
разрыв surrogate pair и значения за пределом LSP uinteger отклоняются.
BOM, CRLF, LF, CR и non-BMP символы учитываются без копии всего текста.
Optional context с triggerKind 1–3, workDoneToken и partialResultToken принимаются;
потоковые части и progress не отправляются.

Ответ — CompletionList с `items`, `isIncomplete` и `eska`. Расширение содержит
актуальный snapshot, source, source/config SHA-256, `position_byte`, текущие
resources и coverage: metadata_inputs_complete, metadata_models_complete,
module_inputs_complete, semantic_complete. Последний пока всегда false.
`isIncomplete=true`, если модели метаданных или входы модулей неполны.
Lazy-чтение целевого BSL и [адресная подготовка метаданных](analyzer-targeted-completion.md)
могут изменить ревизию; ответ сообщает новую. Сервер подготавливает YAML своего
модуля, его primary resource/текущий dialog и прямой manager target, определённый
native ядром. Полный metadata scan не выполняется.

Open/change/close YAML и BSL меняют тот же host: имя реквизита из черновика
заменяет сохранённое в подсказках; неверный текущий YAML очищает модель;
close восстанавливает фактические saved bytes. Старый YAML и XML не служат fallback.

## Представление кандидатов и управление запросом

[Стандартное completion API](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/language/completion.md)
используется для label/kind/insertText/insertTextFormat и optional sortText/filterText.
Типы кандидатов берутся из ядра. Без client `completionItemKind.valueSet`, в том
числе при пустом `completionItemKind: {}`, константы и элементы перечислений
получают совместимый Value. Переданный массив `valueSet` разрешает расширенные
Constant/EnumMember: по LSP такой клиент умеет обрабатывать и неизвестные ему kinds.
Фильтрацию слова для insertText выполняет клиент.

С `snippetSupport=true` сохраняются шаблоны и tab stops вызовов. Иначе шаблоны
исключаются, а вызов вставляется простым именем: сырые `$0` не попадают в текст.
Literal field names не интерпретируются как snippet. Detail/documentation пока
не передаются: в закреплённом ядре есть фиксированные русские presentation strings.
Язык имён и шаблонов следует diagnostics locale настроек ядра; transport keys,
числовые kinds, coverage и error codes от locale не зависят.

Diagnostics и completion используют общий request control, ограниченный reader
и timer server/checker. Они входят в languageMethods и cancellableMethods вместе
с [definition](analyzer-definitions.md); property plan тоже отменяемый.
`$/cancelRequest` возвращает исходному запросу `-32800` с
analysis_cancelled. Timeout `--timeout-seconds` по умолчанию 300, ноль прекращает
операцию до lazy-чтения. Для completion ошибка timeout — `-32000` с
analysis_deadline_exceeded и текущим snapshot; retriggerRequest отсутствует.
`-32802`/retriggerRequest=false остаются специальным ответом diagnostic pull.
Первый stop cause сохраняется. Байты и document version редакторского BSL
сохраняются; принятые до остановки metadata inputs учитываются в актуальном
snapshot ответа. Новый запрос получает новый control. Остановка кооперативная,
без SLA ответа.

## Первоначальная приёмка и ограничения

34 unit/file, 28 analyzer process и 9 checker process tests прошли вместе с
19 gates tool feature matrix, Rustdoc и release builds. Root: 887 passed /
14 прежних ignored; minimal 161/3, project-cli 177/3, build-cli 196/3.
Кандидаты реального процесса совпали с библиотечным ядром в RU/EN; проверены
current YAML/BSL, invalid/close, cursor errors, stale snapshot, capabilities,
snippet support, disabled ParseError, отмена и нулевой timeout.

На C1/C2/C3 в RU/EN воспроизведены saved field → YAML overlay → invalid → close
и смена типа получателя в BSL overlay. Позиция включала BOM, CRLF и non-BMP.
Активное автодополнение синтетического BSL в реальном проекте (3600053 bytes)
после наблюдаемого начала вычисления отменилось за 0,945–0,982 с. Следующий
маленький запрос в том же процессе прошёл; late cancel не повредил ID reuse.
В C1 модуль 4080049 bytes с timeout 1 с дал `-32000` за 1,101–1,113 с.
Это измерения стенда, не гарантии времени ответа.

| Corpus | Saved candidates EN/RU | Peak RSS EN/RU, KiB |
|---|---:|---:|
| C1 | 47 / 47 | 977836 / 982880 |
| C2 | 37 / 37 | 1141344 / 1135132 |
| C3 | 37 / 37 | 1168632 / 1161912 |

PATH пустой, normal exit не ждал stdin EOF. Четыре проверенных файла каждого
corpus — manifest, configuration, выбранные YAML/BSL — сохранили исходные bytes;
остальные файлы не хешировались. Временные settings C2/C3 удалены. Сравнение
локалей проверило source hash, position, coverage/resources и число кандидатов;
полное равенство presentation labels не заявляется.

Release server — 47990472 bytes; runtime graph — 206 package names / 214 resolved
versions. [Машинный отчёт](analyzer-completion-results.json) содержит gates и замеры.
Платформа 1С не запускалась: проверены подсказки, не исполнение BSL.
На commit `af6dd54` resolve, точные textEdit, edit plans, watcher, multi-project
host, полная семантика/metadata coverage и общий heap/RSS budget оставались
в работе. Позднее подключён [первый план правки свойства](analyzer-property-plans.md).
EF-18 остаётся `IN_PROGRESS`; установленная поставка и Explorer не переключены.
