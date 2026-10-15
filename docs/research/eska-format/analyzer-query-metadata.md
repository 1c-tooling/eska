# Диагностика SDBL по текущим метаданным

Первая часть EF-27 подключает standalone native SDBL rules к общей сессии
IDE/checker. Запрос использует текущую модель CF или явно настроенной CFE,
включая видимые зависимости. Параметры, поля результата и потребители СКД
остаются следующими частями; чистый результат не означает полную семантику.

## Контекст и текущий ввод

`FileQueryResolver` получает существующий FileId описателя владельца запроса.
Object/register/defined-type lookup использует прежнюю native per-file visibility:
никакого синтетического BSL-файла и поиска через все корни нет. CFE видит только
объявленную цепочку base/dependencies/selected extension, без соседних расширений.

Перед запросом host подготавливает текущие видимые metadata/resource inputs.
Эта отдельная операция не загружает BSL bodies. Ready editor input и его ошибки
сохраняют authority; pending, invalid или unavailable поддерживаемый вклад
отказывает с `analysis_composition_incomplete`, не возвращаясь к прежней базе.
Standalone CFE, EPF и ERF без owning configuration не получают этот контекст;
прежняя проверка синтаксиса остаётся доступной.

Plain query использует общий revision, source quota, hash, origin, overlay,
watcher и close/replay. Cancellation проверяется до/после native работы,
resolver также использует Salsa cancellation token. Закреплённый parser не
предоставляет внутреннего cancellation hook: немедленное прерывание внутри
parse не обещается. Проверка всех disabled rules и нулевого deadline происходит
до lazy reads. Диапазоны проверяются по тем же текущим UTF-8 bytes и переводятся
в UTF-16 без изменения native diagnostic codes/severity.

## IDE

Новая независимая опция initialize:

```json
{"initializationOptions":{"eska":{"protocolVersion":"0.1","queryAnalysis":true}}}
```

`experimental.eska.queryAnalysis` сообщает negotiation, enabled, native inventory
rules, `scope: "current_canonical_metadata"`, UTF-16 и требование текущих видимых
inputs. Наличие capability не подтверждает readiness конкретной композиции.
Неправильный тип опции отказывает до инициализации.

`textDocument/diagnostic` выбирает маршрут через
`identifier: "eska-query-analysis"`. Если согласован только новый маршрут,
отсутствующий identifier также выбирает его. При одновременном `querySyntax`
прежний default и `eska-query-syntax` остаются parse-only; старый клиент не
получает новых rules без своего согласования.

Результат связывает snapshot, owning root, source hash/revision/origin,
settings SHA и composition SHA. Coverage раздельная:

```json
{"query_syntax_complete":true,"metadata_complete":true,
 "parameters_complete":false,"consumer_complete":false,"semantic_complete":false}
```

`query_syntax_complete` зависит от включения `QueryParseError`.
`metadata_complete` подтверждает admitted current metadata inputs, а не полноту
параметров, выходных типов или потребителей. Для неизвестного/unread input
ошибка сохраняется вместо пустого успешного результата.

Native dispatcher остаётся источником списка rules. Русские сообщения сохранены;
английские для quality/metadata rules выбираются по типизированному diagnostic
code. Parse errors используют прежний structured English translator. Локаль
не меняет code, range, severity или JSON keys.

## Checker

Syntax profile сохраняет прежние query serialization и parse-only правила.
Semantic profile вызывает тот же текущий query API, записывает
`query_files[].scope: "current_canonical_metadata"` и отдельный `context`
с root/input revision/composition SHA/metadata completeness. Query coverage
добавляет `parameters_complete: false` и `semantic_complete: false` только для
semantic profile. Consumer completeness остаётся false.

Общий semantic checker продолжает свой BSL/embedded-query проход и может читать
BSL. Отсутствие BSL reads относится к отдельному standalone query request,
не ко всему checker. Неполные metadata отражаются как unavailable query file;
наличие declared queries сохраняет incomplete/exit 2, пока consumer semantics
не завершена. IDE и checker сравниваются для одного saved input и settings.

## Проверки первой части

Полный SDK: 190 default tests; all-features — 238 library, 198 analyzer process,
30 checker process, 7 navigation и 4 read-permission tests, всего 477.
Fmt/check/strict all-target Clippy и strict Rustdoc прошли для default/all-features.
Native resolver — две новые группы, kernel fmt/check/strict Clippy/Rustdoc прошли.
Две неоднозначные/неразрешённые Rustdoc ссылки исправлены без изменения поведения.

Focused проверки охватывают current metadata replacement при неизменном query,
CFE visibility и sibling exclusion, invalid/failed overlay без saved fallback,
watch/close/repair, parse-only compatibility, disabled/malformed negotiation,
отмену/reused request ID, точные CRLF/non-BMP UTF-16 ranges и RU/EN machine parity.
Saved IDE/checker parity проверяется через настоящий framed transport.
Это синтетические Queries-проекты; native-origin и installed-client результаты
фиксируются отдельно. [Обезличенные результаты](analyzer-query-metadata-results.json).

Следующий прогон на immutable release binary проверил четыре native-origin
случая с пустым PATH. CF на RU/EN дал одинаковые saved IDE/checker codes/ranges/hash;
объявленное поле разрешилось, dirty query с отсутствующим полем дал
`UnknownFieldInQuery`, close восстановил saved результат. Standalone request
сохранил `module_text_bytes: 0`.

У явно настроенной CFE оба языка отказали одинаково: current input одного
CommonPicture разобран, но его модель пока Unsupported. IDE вернул
`analysis_composition_incomplete`, checker — unavailable с теми же code/location
и exit 2, без выдуманных findings. Синтаксический маршрут сохранён. Совместимость
этой native CFE с выбранной базой не установлена; успешной CFE semantic приёмки
нет. Все дочерние процессы завершены, source/authority bytes и mtime сохранены.

Полный прогон выявил прежнюю гонку нулевого timeout: timer thread мог ещё не
успеть отменить input до первого semantic preparation. Checker теперь синхронно
проверяет deadline перед этим чтением. RU/EN regression требует одинаковых
`metadata_pending` и отсутствующего source hash; тест не нормализует различие.

## Границы

Этот маршрут проверяет native syntax/quality/metadata rules. Он ещё не строит
полный контракт выходных полей и параметров, не анализирует текущий ресурс СКД,
dataset links, calculated/total fields, сохранённые настройки и UI DataPath.
Динамические BSL-строки, исправления потребителей и fmt не добавлены.
Полная EF-18/EF-27, installed Explorer semantics и MVP остаются в работе.
