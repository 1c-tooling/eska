# Переход к объекту и свойству текущего YAML

`eska-analyzer` принимает `eska/metadata/locate` для готового владельца YAML.
Клиент передаёт точный snapshot, source path, UUID объекта и необязательный
native QName свойства. Ответ содержит обычный Location с UTF-16 range,
текущие source SHA-256, origin, native path и исходный byte range.

## Общая граница

`SourceReader::parse_metadata` сохраняет только выбранные границы объектов
и свойств. UTF-16 вычисляется одним проходом по тому же тексту после разбора
CST. `MetadataSnapshot::coordinates` не хранит исходную строку, XML, AST или
дополнительный индекс UUID. `TextCoordinates` теперь также обслуживает диапазоны
диагностики и определений BSL вместо второго конвертера в transport.

`AnalysisSession::metadata_location` ищет UUID только в явно указанном текущем
владельце. Inline-объект и внешний вложенный descriptor открываются в физическом
YAML этого владельца; native path объясняет происхождение, но не становится
URI сгенерированного XML. Свойства выбираются по namespace и name, а не по
текстовому совпадению. Диапазон относится ко всему object mapping или property
entry существующего CST; отдельный selection range значения не придумывается.

Запрос не читает диск, не загружает pending inputs, не изменяет revision или
resources. Клиент сначала вызывает `eska/metadata/read`, `refresh` либо передаёт
черновик через document sync. Для дерева доступна и корректная YAML-модель вида,
который ещё не поддержан канонической BSL-проекцией. Настройки BSL, analysis
timeout и pin принятой базы не нужны для этой операции чтения.

## Протокол

Initialize добавляет capability `experimental.eska.metadata.locate`:

```json
{
  "method": "eska/metadata/locate",
  "scope": "current-ready-owner",
  "positionEncoding": "utf-16",
  "lazyRead": false
}
```

Пример params после загрузки нужного владельца:

```json
{
  "snapshot": {"session_id": "opaque-session", "revision": "1"},
  "source": "CommonModules/Tools/object.yaml",
  "uuid": "20000000-0000-4000-8000-000000000001",
  "property": {
    "namespace": "http://v8.1c.ru/8.3/MDClasses",
    "name": "Server"
  }
}
```

Поле property можно опустить для перехода к объекту. Namespace null/отсутствует
означает именно отсутствие namespace; это не wildcard. UUID сравнивается без
учёта регистра; QName сохраняет точное сравнение. Неизвестные поля params и
property отклоняются. Result содержит `snapshot`, `source`, `source_sha256`,
`origin`, `uuid`, `native_path`, `byte_range`, `location`, `resources`.
`origin` имеет общий вид saved либо overlay с document_version.

Location использует нулевые координаты и конец диапазона вне выделения,
согласно [Position LSP 3.18](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/position.md)
и [Range](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/types/range.md).
Прежний `metadata/read` сохраняет свои UTF-8 byte ranges; `locationEncoding`
этого DTO не меняется. CRLF, LF, CR и non-BMP учитывает общий конвертер.
BSL с BOM допустим; YAML ESKA по-прежнему отклоняет BOM.

Pending даёт `-32000`/metadata_input_pending, invalid/unavailable —
metadata_input_unavailable с текущим failure. Отсутствующий UUID или QName —
`-32602`/metadata_object_not_found или metadata_property_not_found.
Неоднозначность не даёт успешной случайной цели. Неверный snapshot возвращает
общий `-32801`; неверные wire fields/source/UUID — `-32602`.
Ошибка внутренних координат — `-32603`/metadata_coordinates_inconsistent.
Нет пустого успешного ответа вместо неготовой или отсутствующей цели.

## Проверки

Прошли 39 unit/file, 42 analyzer process и 9 checker process tests, все 19 tool
gates для четырёх feature profiles, Rustdoc и release. Root: 892 passed /
14 прежних ignored; minimal — 164/3, project-cli — 180/3, build-cli — 199/3;
все 15 gates прошли. Новые проверки охватывают flow YAML с non-BMP в значении,
inline/nested объекты, Summary/All, произвольные QNames, strict params, pending,
неверный snapshot, invalid/close и неподдержанный BSL kind.

В process test удалён только собственный fixture file после загрузки: locate
вернул прежний текущий снимок без чтения файла. Черновик заменил координаты,
YAML с BOM очистил их, close восстановил сохранённый снимок. Клиентский код
независимо вычисляет SHA-256 и UTF-16; ответы совпадают в RU/EN.

На C1/C2/C3 прошли 18 сценариев и 1116 запросов locate. В каждом корпусе
проверены общий модуль, справочник с вложенными descriptors и роль, в RU/EN,
для saved/editor/close. Проверены все объекты выбранных владельцев, Name и ещё
одно свойство каждого объекта, включая preserved XML-представление свойства.
Комментарий с non-BMP и CRLF меняет только editor input; invalid убирает старую
цель. Zero timeout не мешает metadata query, BSL texts остаются незагруженными.
Machine traces совпали после нормализации session identity.

| Corpus | Объекты выбранных владельцев | Native parts | Peak RSS, KiB |
|---|---:|---:|---:|
| C1 | 32 | 5 | 38800–49548 |
| C2 | 25 | 5 | 173540–174456 |
| C3 | 5 | 4 | 196464–197032 |

RSS относится к ограниченному сценарию, не полному анализу. Три проверенных
файла каждого сценария — manifest, configuration и owner YAML — сохранили
байты; остальные файлы отдельно не хешировались. PATH пустой, normal exit не
ждал EOF; временные project settings собственных C2/C3 удалены. Настоящий
VS Code и платформа 1С в этом компоненте не запускались.

Отдельная полная проверка семантической проекции All прошла после изменения
parser: C1 — 3595 descriptors / 8330 объектов за 7,19 с; C2 — 39572 / 103417
за 128,68 с; C3 — 45708 / 131381 за 171,25 с. Все логические поля совпали с
исходным XML, metadata diagnostics — ноль. Native и YAML диапазоны исключены
из этого сравнения: они относятся к разным текстам; точные UTF-16 отдельно
проверены в сценариях выше. Время включает независимый XML-обход и обе проекции,
это не время IDE-запроса. BSL/ресурсы и платформа в этой проверке не участвуют.

Release server — 48801256 bytes; runtime graph сохранил 206 package names /
214 resolved versions. Dependencies, CLI options и config keys не добавлены.
В экспериментальный Rust `MetadataSnapshot` добавлено поле coordinates;
это изменение внутреннего API, не новая стабильная версия SDK.
[Машинный отчёт](analyzer-metadata-locations-results.json) сохраняет gates и замеры.

`textDocument/definition` пока ищет только BSL-объявления. Связь BSL-символа с
UUID метаданных, отдельный selection range, полные IDE edits, memory budget,
watcher и переключение Explorer остаются в работе. EF-18 — `IN_PROGRESS`.
