# Отчёт памяти текущей сессии анализатора

`eska-analyzer` предоставляет `eska/analysis/memory` для явно запрошенного
снимка текущих таблиц ядра. Это потребитель существующего
[`AnalysisSession::kernel_memory()`](kernel-memory.md), а не отдельный
анализатор, индекс или сборщик памяти. Запрос не вычисляет BSL queries,
не читает файлы, не загружает pending inputs, не меняет ревизию и не очищает кеши.

## Протокол

После initialize/initialized capability `experimental.eska.kernelMemory`
объявляет method `eska/analysis/memory`, scope `current-session`, lazyRead=false,
heapEstimateComplete=false и processRss=false. Она доступна без accepted pin,
при повреждённых BSL settings и нулевом query timeout. Метод не добавлен
к девяти languageMethods или десяти cancellableMethods; отдельного worker,
background sampler, progress и кооперативного deadline у него нет.

Params содержит только обязательный `snapshot` с UUID текущей сессии и её
десятичной строковой revision. Неизвестные поля отклоняются; чужая сессия даёт
unknown_session, устаревшая revision — stale_revision. Запрос подчиняется общему
жизненному циклу сервера, включая отказ после restartRequired.

Ответ имеет schema_version=1. SDK revision не сериализуется вторым числовым
полем: межпроцессный clock находится только в `snapshot`.

| Поле | Значение |
|---|---|
| snapshot | Текущие session_id и строковая revision |
| resources | Прежние счётчики и квоты текущих YAML/BSL inputs |
| kernel.tables | Все наблюдаемые таблицы без объединения одинаковых имён |
| kernel.tables_without_heap_hook | Число таблиц с heap_bytes=null |
| kernel.live_tables_without_heap_hook | Такие таблицы с entries>0 |
| kernel.summed_heap_estimates | Сумма доступных heap_bytes, bytes |
| heap_estimate_complete | false |
| process_rss_bytes | null, измерение RSS не реализовано |

Каждая таблица содержит `name`, `entries`, `metadata_bytes`,
`field_stack_bytes`, `heap_bytes`. Счётчики — неотрицательные числа;
heap_bytes допускает null. Null означает недоступную aggregate-оценку;
0 может означать полученную нулевую оценку или вытесненные значения.
Entries считает instances, включая memo без сохранённого значения.
Даже ненулевой aggregate может пропускать values без hook внутри той же таблицы.
Сумма не включает metadata_bytes/field_stack_bytes. Схема ответа не менялась.

Ядро называет таблицы по типам значений; разные ingredients могут иметь
одинаковый name. Потребитель не должен превращать массив в map по этому полю.
SDK упорядочивает таблицы по полной последовательности name, entries,
metadata_bytes, field_stack_bytes, heap_bytes, с null перед числами.
Это устраняет неоднозначный порядок одинаковых имён и не меняет измерения.
Имена отражают закреплённую реализацию и не являются стабильными protocol enums
или идентификаторами таблиц. Размеры могут меняться после вычислений без смены
input revision: snapshot фиксирует исходники, а не состояние allocator/кешей.

## Ограничения измерения

Heap estimates могут повторно учитывать общий Arc, исключают таблицы без hook
и память вне Salsa. Composition, текущие YAML-модели, intern pools, временные
объекты и allocator не покрываются полностью. Сумма не является RSS, точным
heap, верхней границей потребления или доказательством отсутствия утечки.
`resources` считает текущие source bytes; свободная квота не означает свободную
память процесса. Метод не вводит общий memory budget и не предотвращает OOM.

Наблюдение не освобождает parse memo. `didClose` использует прежнее поведение:
читает saved input либо фиксирует его отсутствие. Сам отчёт этого чтения
не вызывает. Если файл удалён при открытом overlay, память и структура текущего
BSL доступны до close; отчёт не восстанавливает удалённый файл.

## Проверки

Три новых process tests в RU/EN проверяют строгую схему, snapshot guards,
capability, повторные cold/warm samples, отсутствие lazy I/O после удаления
сохранённого BSL, close, повреждённые settings и нулевой timeout. Существующий
core test дополнен сравнением независимых одинаково прогретых сессий;
сохранение inputs/диагностики после cache trim проверяется прежним сценарием.

All-features: 106 unit/file, 119 analyzer и 11 checker process tests, всего 236.
Minimal — 84; checker — 86 unit/file и 11 process; analyzer — 106 unit/file
и 118 process. Все 19 tool gates прошли: fmt/check/strict Clippy/test/Rustdoc
с warnings denied в четырёх profiles и обе release-сборки.
Все 15 root gates прошли: 895 passed / 14 прежних ignored; minimal — 165/3,
project-cli — 182/3, build-cli — 201/3. Новых dependencies и human strings нет.
Normal graph обоих инструментов прежний: 206 имён/214 версий; Git transport,
LSP runtime, Tokio и reqwest не добавлены. Release analyzer — 50090504 bytes,
checker — 46269312 bytes. Diff проверен с учётом CRLF.

[Обезличенные измерения](analyzer-kernel-memory-results.json) фиксируют шесть
процессов C1/C2/C3 в RU/EN. В каждый собственный проект скопированы полный
manifest/configuration snapshot и один настоящий CommonModule. Остальные
дескрипторы остаются pending; их тексты не загружаются. Это проверка полного
состава с одним BSL input, а не памяти анализа всей конфигурации.

Каждый процесс получил 501 вариант этого тела с новыми именами только
добавленного собственного метода, выполнил 501 native references и снял
13 memory samples: cold, после каждых 50 изменений и после close.
Всего — 3006 изменений/поисков, 6012 Locations и 78 снимков. Current hashes,
ревизии и bytes независимо подтверждены; machine reports совпали в RU/EN
после замены только session UUID. Результаты не объединялись по имени таблицы.

| Состав | Объявлено модулей | RSS после первого поиска, KiB | RSS после 500 изменений, KiB | Heap estimates после 500 изменений, bytes |
|---|---:|---:|---:|---:|
| C1 | 1975 | 68464–68528 | 69012–69080 | 2894519 |
| C2 | 19170 | 196752–197024 | 197764–198112 | 22270667 |
| C3 | 22710 | 220404–220600 | 221324–221532 | 25509568 |

RSS/VmHWM прочитаны внешним Linux probe и не присутствуют в protocol report.
Пиковый RSS — 69012–221536 KiB; время memory request —
0,000694–0,006310 секунды. Доступные heap estimates выросли на 42 bytes
после первого запроса и с сотого изменения оставались постоянными;
RSS продолжал меняться. Это подтверждает ограниченность estimator, но
не устанавливает причину роста памяти или отсутствие утечек.

12 выбранных оригиналов и девять файлов собственных копий сохранили bytes;
уникальные проекты удалены. Процессы завершились штатно при открытом stdin
и пустом PATH. Workspace MCP недоступен; установленный bsl-analyzer-app 0.3.0
дополнительно проверил SyntaxError на трёх собственных BSL-копиях, exit 0.
Платформа 1С и настоящий IDE-клиент в этой части не проверялись.

Из корня репозитория:

```bash
ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test --manifest-path tools/analysis-core/Cargo.toml --all-features --locked
```

Общий memory budget, полнота моделей/семантики, остальные IDE edits и Explorer
ещё в работе; EF-18 остаётся `IN_PROGRESS`.
