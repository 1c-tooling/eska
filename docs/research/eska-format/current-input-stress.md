# 500 циклов текущих входов и replay после отказа квоты

Проверен checkpoint `2deec7f`: один замороженный `eska-analyzer` debug на Linux
x86_64, отдельные копии ранее собранных и выгруженных платформой EPF/ERF.
В каждом процессе выполнено 500 циклов по трём открытым входам. Для EPF это
owning YAML, object BSL и единая managed form YAML; для ERF — owning YAML,
object BSL и declared `.sdbl`. Каждый тип проверен на RU/EN, всего 6000
обновлений full sync. Runtime PATH пустой.

## Методика

`initialize` явно включает `querySyntax`. После каждого `didOpen`/`didChange`
проверяются общий snapshot и SHA-256 принятого текста. Для YAML вызывается
`eska/metadata/read`, для BSL/query — `textDocument/diagnostic`. YAML/form
получают меняющийся comment, BSL — функцию с новым именем; query чередует
правильный SELECT с синтаксической ошибкой и UTF-8/BOM/CRLF. Query findings
остаются syntax-only, metadata/consumer coverage — `false`.

`eska/analysis/memory` снимается после initialize, первого цикла и каждых
50 циклов. Source reserve проверяется отдельно от native heap estimates и
Linux `/proc` RSS. После shutdown новый процесс получает точные последние
тексты; повторяются current reads/pulls, затем close восстанавливает текущий
диск. Все исходные проекты и их копии сохраняют bytes/mtime. Все 16 children
основного и quota-прогонов завершились.

## Измерения

Таблица показывает диапазоны двух локалей; один цикл меняет все три входа.
Таймер запускается после `Popen`: учитывает handshake, но не сам вызов запуска
и не загрузку всего дерева. Таймер replay также начинается после запуска child.
Системный файловый кеш не очищался. Параллельно выполнялись независимые
Cargo-сборки, поэтому времена не являются изолированным benchmark.

| Проект | Handshake, ms | Первый цикл, ms | Медиана следующих циклов, ms | Handshake и replay нового child, ms | RSS после первого / 500-го цикла, KiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| EPF | 30–36 | 91–113 | 67 | 115–123 | 53564–53676 / 54440–54548 |
| ERF | 31–36 | 36–56 | 43–46 | 64–103 | 53752–53864 / 55516–55704 |

Финальный source reserve — 41414 bytes для EPF и 8718 для ERF. Native heap
estimates — 60641 и 58544 bytes. Source/heap counters совпали на RU/EN во всех
точках; OS RSS и времена сравниваются отдельно. Рост оценок кеша на первых
циклах затем вышел на наблюдаемое плато; 500 циклов не доказывают отсутствие
неограниченного роста на других входах.

## Отказ и восстановление

Отдельные четыре процесса с `--max-retained-source-bytes 0` приняли версии
черновиков как current unavailable с `analysis_source_budget_exceeded`.
Прежний hash очищен, резерв равен нулю; повтор той же document version
отклонён без изменения snapshot. Полные тексты остались у клиента.
Перезапуск с обычной квотой и replay тех же текстов восстановил каждый
YAML/form/BSL/query input и его точный hash без записи исходников.

[Обезличенный отчёт](current-input-stress-results.json) содержит samples и
раздельные quota cases. Это малые проекты и debug build. Большой CF/CFE,
время запуска установленного Explorer,
семантика запросов и общий hard heap/RSS budget ещё требуют приёмки;
EF-18/EF-16 остаются в работе. [Source quota](source-budget.md) ограничивает
байты текущих входов, а не весь процесс.
