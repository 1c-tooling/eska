# Подробные ошибки метаданных в IDE и CI

`eska-check --profile syntax` теперь добавляет конкретные причины к прежнему
списку неполных metadata owners. Сохранённый YAML проверяется тем же
`AnalysisSession::metadata_diagnostics()`, что `textDocument/diagnostic` сервера.
Общий presentation переводит typed причины в machine fields и RU/EN объяснения;
правила не копируются между точками входа.

## Ответственность и контракт

Domain report содержит `MetadataIssue.diagnostics: Option<Vec<MetadataDiagnostic>>`.
Это typed данные без JSON/Fluent; поле исключено из общего `Serialize` отчёта.
CLI presentation добавляет `findings` в JSON каждого metadata issue. Общий
presentation включается только features `check-cli`/`analyzer-cli`; минимальная
библиотека остаётся без CLI и транспорта. Новых dependencies нет.

Прежние source/hash, aggregate code, location и byte range сохранены.
Подробный finding содержит `range` в UTF-16, numeric `severity`, стабильный `code`
и `data` с `start_byte`, `end_byte`, `native_path`, `issue`. Это в точности IDE item
без полей отображения `message` и `source`. Различаются ошибки adapter,
recoverable parse issues и неподдержанный kind; QName и значения не нормализуются.
Parse issue вложенного описателя сохраняет его native path и диапазон
в текущем физическом owning YAML.

`data.position_encoding=unicode-scalar` сохранено для BSL. Дополнительное
`data.metadata_position_encoding=utf-16` относится к metadata findings.
JSON не содержит локализованных сообщений; human output показывает общие
объяснения и позиции от 1. Ошибки обязательных данных имеют severity 1,
неподдержанное покрытие — 2, по тем же правилам, что IDE.

`findings=null` означает незавершённую подробную проверку текущего входа:
pending, unavailable или отмена. `[]` — завершённая локальная проверка без
конкретной причины; неполнота вложенных моделей при этом остаётся в aggregate
code/coverage. Полностью исправленный owner исчезает из metadata issues.
Отсутствующий, синтаксически некорректный YAML и refusal по квоте не получают
выдуманного диапазона. Ошибка координат/инвариантов остаётся fatal; отмена
сохраняет incomplete report и stop, без успешного пустого результата.

Schema 1, профиль `syntax`, exit 0/1/2/3 и протокол сервера 0.1 сохранены.
Неполнота имеет приоритет над findings. Некорректные настройки BSL по-прежнему
дают checker exit 3 до анализа; независимый metadata provider IDE не менялся.
Это не новый semantic profile и не проверка всех ограничений платформы 1С.

## Проверки

75 unit/file, 74 analyzer process и 11 checker process tests прошли: всего 160;
minimal — 60, самостоятельный checker — 62 unit/file и 11 process tests.
Core batch проверен против resident query для invalid property, nested parse
issue с готовой главной моделью и unsupported role. Отмена сохраняет incomplete
owner без fake findings. Checker process tests RU/EN проверяют причины, hash,
bytes/UTF-16, malformed/missing/correction и human output. В compact JSON non-BMP
символ находится перед диапазоном на той же строке: UTF-16 действительно
отличается от Unicode scalars. Ещё один process-тест сравнивает IDE/CI wire results.

Все 19 tool gates и 15 root gates прошли: fmt/check, строгие Clippy/Rustdoc,
тесты четырёх tool profiles и release builds. Root — 893 passed/14 прежних ignored;
minimal 165/3, project-cli 181/3, build-cli 200/3. После усиления non-BMP fixture
повторены все tool gates; production-код не менялся. Ключи RU/EN совпадают.

На C1/C2/C3 выполнены 12 сценариев RU/EN с неверным и исправленным `Server`.
Составы полные, payload копий намеренно ограничен; один активный BSL-модуль
проверен в каждой копии, binary и missing modules остаются явными. 32 IDE pulls
подтвердили 26 точных совпадений findings с CI: property error, реальные роли,
два существующих неподдержанных `cfg:AnyRef` C1 с исходным QName.
Исправление удалило property error из обоих результатов. JSON RU/EN одинаков,
human explanations локализованы. Всего запущено 20 checker processes:
12 partial JSON, шесть human и два полных C1 JSON.

Полный saved sweep C1 проверил все 1 975 активных текстовых модулей и 2 645 YAML
owners. Syntax findings — 0, input coverage полное. 1 692 owners сохраняют
неполноту моделей; для них выполнена подробная проверка, получено 1 375 findings:
1 373 unsupported kinds и два unsupported types. У 317 owners `findings=[]`
сохраняет aggregate `metadata_descriptors_unmodeled`. Exit 2 сохранён.
Все 7 829 исходных файлов проверены по hash, байты не изменены.

Граф каждой tool feature — 206 package names/214 resolved versions; analyzer
graph byte-identical прежнему. Нет LSP runtime, Git engine, платформенного
runner или отладчика. Release checker — 46 252 536 bytes, analyzer — 49 141 912.
Параметры запусков, времена, RSS и результаты — в [машинном отчёте](analyzer-checker-metadata-results.json).

Bytes 7/5/5 выбранных оригиналов и копий сохранены; временные проекты удалены.
У tools был пустой PATH, native XML отсутствовал, сервер завершился без stdin EOF.
Реальный клиент VS Code, полный проход C2/C3, runtime 1С, полная семантика и общий
heap/RSS budget здесь не проверялись. EF-18 остаётся `IN_PROGRESS`.
