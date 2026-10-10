# Переход из BSL к текущему объекту YAML

Первый компонент на commit `a13d796` проверил переход к верхнеуровневому объекту.
Текущие дочерние цели и capability описаны в [member definitions](analyzer-member-definitions.md).
Counts и замеры ниже относятся к первому компоненту.

`textDocument/definition` открывает объект метаданных в его физическом
YAML. Для `Справочники.Sample` или переменной с типом коллекции анализатор
подтверждает объект по текущей канонической модели, UUID и координатам того же
снимка. Диапазон относится ко всему object mapping; отдельный selection range
не добавляется.

## Общая ответственность

`AnalysisSession::module_metadata_definition` использует CST и вывод типа
receiver из закреплённого ядра. Только тип ManagerCollection разрешает переход
к объекту соответствующего семейства. Имя поля выбирается из AST и ищется
в существующем ConfigIndex без учёта регистра. Текстовый поиск `Справочники.`
или ручной список псевдонимов платформы не нужен. Имена локальных `Перем`,
параметров и переменных модуля учитывает то же ядро, что автодополнение.

Каноническая модель владельца должна быть ready. FileId главного descriptor
разрешается через существующий SourceRoot/FileSet, а объявленный native path —
через `SourceReader::owner_for_native_main`. Затем общий `metadata_location`
находит UUID в текущем YAML и возвращает его точные координаты. Нового обратного
индекса, чтения XML, копии YAML или отдельного parser в transport нет.

Pending, invalid, incompatible и unsupported модели не подтверждают старую
цель. Соседние владельцы автоматически не загружаются. Клиент явно читает YAML
или передаёт document overlay. Caller BSL по-прежнему может загрузиться лениво;
в результате указывается фактическая ревизия после чтения.

На первом шаге проверены справочники, константы, перечисления и регистры сведений.
Вложенные реквизиты/значения перечисления, строки запросов, идентификатор
пространства общего модуля и полная семантика не входят в этот компонент.
Для вложенных объектов уже доступен отдельный [metadata locate](analyzer-metadata-locations.md).
Переходы к [BSL-процедурам](analyzer-definitions.md) сохранены.

## Протокол

Initialize при корректных настройках анализа объявляет definitionProvider=true
и обновлённую capability:

```json
{
  "scope": "ready-bsl-and-canonical-metadata",
  "metadataTargets": "top-level-objects",
  "incompleteResults": "error"
}
```

Params остаются стандартными: URI объявленного BSL, UTF-16 position,
необязательный `eska.snapshot` и progress tokens. Подтверждённая цель — обычный
[Location LSP 3.18](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/language/definition.md).
URI указывает на YAML владельца, range относится к текущему YAML input,
а caller hash — к текущему BSL input. Они не подменяют друг друга.

В `result.eska.target` для метаданных добавлены `kind: "metadata"`, UUID,
native_path и origin; source/hash/start_byte/end_byte сохраняются. Origin
указывает saved либо overlay с document_version. Форма обычной BSL-цели
не меняется. Общие snapshot, coverage и resources остаются независимыми от locale;
semantic_complete пока false. Неподтверждённая цель при неполных входах даёт
navigation_incomplete, а не успешный пустой диапазон или XML fallback.

Отмена, deadline, snapshot validation и frozen settings используют прежние
общие механизмы. Запрос работает с тем же control, что обычный BSL definition;
новой очереди, option или config key нет. Ошибка BSL settings не блокирует
самостоятельный metadata locate.

## Проверки

Прошли 42 unit/file, 44 analyzer process и 9 checker process tests. Все 19 tool
gates четырёх feature profiles, Rustdoc и release прошли. Root: 892 passed /
14 прежних ignored; minimal 164/3, project-cli 180/3, build-cli 199/3; все 15 gates
прошли. Проверены текущая YAML-цель без disk read, сохранение BSL-переходов,
alias, case folding, declared local/parameter/module shadowing, invalid/close,
stale, UTF-8/UTF-16, отмена и прежние timer/ID contracts.

Присваивание `Справочники = Новый Структура` само по себе не объявляет локальную
переменную: это запись в read-only property. Карточка API через подключённый
bsl-analyzer-reference подтверждает read_only=true. Тест отличает такую запись
от явного `Перем Справочники`; специальный обход семантики ядра не добавлен.
Повторный запуск платформы для этого утверждения не выполнялся.

На C1/C2/C3 в RU/EN прошли 24 сценария четырёх семейств: 96 подтверждённых
definition и столько же независимых locate. С учётом pending/stale/invalid и
трёх shadowing-отказов выполнено 240 definition. Saved/editor/close, alias,
Unicode case folding и non-BMP/CRLF дали точные текущие hash/byte/UTF-16 ranges.
Machine traces совпали после нормализации opaque session identity.

| Corpus | Первый saved query, мс | Peak RSS, KiB |
|---|---:|---:|
| C1 | 0,499–0,706 | 66856–67464 |
| C2 | 0,525–0,758 | 197172–198176 |
| C3 | 0,606–0,841 | 221784–222848 |

Это ограниченный сценарий с маленьким editor BSL и явно прочитанным владельцем,
не полный анализ конфигурации и не SLA. Проверенные manifest, configuration,
owner YAML и saved BSL каждого сценария сохранили bytes; остальные файлы
отдельно не хешировались. PATH пустой, normal exit не ждал stdin EOF, временные
settings собственных C2/C3 удалены. Платформа 1С и настоящий VS Code client
в этой проверке не запускались.

Прямой `hir` dependency закреплён на том же upstream revision; он уже был
транзитивной зависимостью ядра. Runtime graph сохранил 206 package names /
214 resolved versions. Release server — 48818752 bytes.
[Машинный отчёт](analyzer-metadata-definitions-results.json) содержит gates и замеры.

EF-18 остаётся IN_PROGRESS: вложенная BSL-навигация, все IDE edits, полнота
анализа, общий memory budget, watcher и Explorer ещё не завершены.
