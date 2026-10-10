# Планы правки свойств YAML

`eska-analyzer` возвращает проверенный план изменения существующего свойства
открытого YAML. Общая сессия проверяет текущий ввод, а редактор EF-07 —
поддержку, принятую identity и lossless CST-правку. Сервер не записывает файл
и не отправляет workspace/applyEdit: применение остаётся обязанностью клиента.

## Включение и граница доверия

```bash
eska-analyzer --project-dir . --accepted-base-sha256 <approved-base-sha256>
```

Вместо option допускается `ESKA_ACCEPTED_BASE_SHA256`; явный option имеет приоритет.
Используется общий resolver project/build CLI: ровно 64 hexadecimal characters,
нормализация к нижнему регистру. Значение должно быть одобрено отдельно от
редактируемого `eska.toml`; его accepted_base задаёт только местоположение.
Некорректный pin отклоняется при initialize. CLI help локализован на обоих языках.

Клиент должен объявить `capabilities.workspace.workspaceEdit.documentChanges=true`.
Тогда initialize сообщает `experimental.eska.editPlans=true` и
`propertyEditPlans={method:"eska/metadata/preparePropertyEdit",scope:"open-document",versioned:true}`.
Без pin или поддержки versioned edits capability false, запрос даёт
source_edit_plans_disabled. Initialize не читает базу; отсутствующее или неверное
хранилище выявляется при подготовке. Дерево и языковые запросы не требуют базы.

## Запрос и результат

После initialize/initialized и didOpen YAML запрос содержит:

```json
{
  "snapshot": {"session_id": "<current-session>", "revision": "<current-revision>"},
  "source": "CommonModules/Tools/object.yaml",
  "document_version": 1,
  "text": "<exact-current-editor-text>",
  "uuid": "20000000-0000-4000-8000-000000000001",
  "property": "Comment",
  "value": {"state": "text", "value": "Updated comment"}
}
```

Snapshot обязателен. Источник должен быть объявленным ready YAML с активным
overlay. Версия — текущий signed 32-bit version из document notifications.
Полный text нужен для CST; сессия хранит логическую модель и fingerprint,
а не второй редакторский буфер. До baseline IO проверяются revision, origin,
version, размер и SHA-256 текста. Invalid/pending input, закрытый документ,
другие bytes или версия не дают правку. Fallback на сохранённый файл запрещён.
Запрос ограничен общим wire limit 4 MiB, включая JSON-экранирование.

Затем `AcceptedBase::open` заново проверяет независимо закреплённый manifest
и required blobs. UUID, owner и ancestry берутся из принятой базы. Политика
supplier и дополнительная freeze ESKA сохраняются; Name/создание/удаление/
rename/move не включены. Поддержаны только существующие typed Comment, Version
и Synonym в прежнем scope EF-07, включая проверенные состояния и языки.

Результат содержит snapshot, source, source_sha256 исходного editor text,
document_version, accepted_base.sha256, unchanged и workspace_edit. Последний —
стандартный [WorkspaceEdit](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/workspaceEdit.md)
с одним [TextDocumentEdit](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/textDocumentEdit.md):
URI текущего YAML, обязательная версия и UTF-16 range/newText.
Неизменённый prefix/suffix исключены из правки; Unicode scalars и CRLF остаются
целыми. No-op содержит пустой edits и сохраняет исходное написание.

Клиент проверяет session/revision и применяет versioned edit к тому же документу.
Последующий didChange обновляет общий host; получение плана само по себе
не меняет input/revision/quota. План после смены версии нельзя применять к
новому документу. Реальный VS Code client ещё не подключён: проверена серверная
структура и применение в памяти тестового клиента, не runtime расширения.

## Общие API и ошибки

`prepare_property_edit_text` и `SourcePropertyEdit` добавлены в project core
для caller-owned text. Они используют тот же permission/ancestry/CST/validation
код, что saved `prepare_property_edit`; preview не имеет publish-метода.
Прежний saved writer повторно проверяет исходные bytes и базу перед записью.
`AnalysisSession::property_edit_input` связывает editor bytes с текущей сессией;
`PropertyEditInput::prepare` возвращает plan с revision/version/hash и preview.
Этот borrowed input живёт только во время подготовки, plan cache отсутствует.

Ошибки version/hash — `-32801` с source_edit_document_version/
source_edit_input_conflict; stale snapshot сохраняет прежний контракт.
Closed/unavailable input, disabled plans, policy/value/base failures имеют
`-32000` и структурированный code/location. Params/schema errors — `-32602`.
Ни один отказ не возвращает пустой успешный plan и не заменяет текущую модель.

Метод пока не входит в cancellableMethods и не использует language deadline:
полная проверка базы не имеет кооперативных checkpoint. Очередь остаётся общей
и ограниченной. Адресная проверка доверия policy reader и управление временем
этого IO требуют отдельного следующего компонента; кеш без проверки не добавлен.

## Проверки

35 unit/file, 33 analyzer process и 9 checker process tests прошли вместе с
19 gates tool feature matrix, Rustdoc и release builds. Root: 888 passed /
14 прежних ignored; minimal 161/3, project-cli 177/3, build-cli 196/3.
Одиннадцать прежних и новых source-edit integration scenarios подтвердили
сохранение прежнего saved поведения. Проверены core/server parity, обе локали,
current text, no-op, readonly disk/input, preflight до отсутствующей базы,
freeze, неподдержанные операции, неверный pin и повреждённый accepted manifest.

В C1 pin взят из отдельно сохранённого результата прежнего import. В RU/EN
получен и применён в памяти versioned план разрешённого Comment; didChange
обновил host, stale/version/hash отклонены, no-op пустой, close восстановил saved.
Настоящий supplier restriction дал source_edit_native_restricted. Полные
machine traces совпали после нормализации только opaque session identity.

| Locale | Initialize, с | Plan / no-op / native refusal, с | Peak RSS, KiB |
|---|---:|---:|---:|
| EN | 0,070 | 4,170 / 3,444 / 3,538 | 102044 |
| RU | 0,071 | 3,652 / 3,643 / 3,650 | 101716 |

Полная повторная проверка базы — измеренный расход этого первого компонента,
а не постоянный расход дерева. PATH пустой; normal exit не ждал stdin EOF.
Пять проверенных файлов — source manifest/configuration, два YAML и accepted
manifest — сохранили bytes; остальные файлы отдельно не хешировались.
C2/C3 не использовались для этой проверки: требовалась готовая независимо
закреплённая база C1. Платформа 1С не запускалась: проверен plan, не его исполнение.

Release server — 48476320 bytes; runtime graph — 206 package names / 214 resolved
versions. [Машинный отчёт](analyzer-property-plans-results.json) содержит gates и замеры.
Это первый property plan, не готовый механизм всех IDE edits. Saved-document
plans, code actions/refactorings, адресная policy verification, полный memory
budget, semantic/metadata coverage и Explorer остаются в работе. EF-18 не завершена.
