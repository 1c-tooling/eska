# Очистка native-кешей после языкового запроса

Ранее IDE выполняла глубокий LRU sweep только после успешной semantic
диагностики BSL. Completion, hover, навигация, структура модуля, rename и
ошибочный diagnostic pull могли оставлять вычисленные значения в более
широком interactive окне. Пакетный checker имел свою очистку каждых 32 модулей
и в конце прохода.

## Текущий контракт

Общий `request_control::finish_language` выполняет существующий
`AnalysisSession::trim_batch_caches()` после завершения native вычислений,
для `Ok` и `Err`. Короткие `Analysis`/query-registration handles уже уничтожены:
Salsa получает исключительный доступ к базе. Тот же живой `Deadline` остаётся
активным во время sweep; после очистки повторно проверяется причина остановки.
Поздний успех не скрывает deadline или отмену.

Общий выход используют все девять language methods: diagnostic, completion,
definition, documentSymbol, hover, signatureHelp, references, prepareRename и
rename. Diagnostic включает metadata YAML и declared query syntax/analysis.
Отказ до запуска управляемого запроса не создаёт новых native вычислений.

Очистка уменьшает прежние LRU окна и удаляет saved parse hints. Текущие входы,
source hashes, document versions, revision и source reserve сохраняются.
Parse hints открытых BSL overlays остаются; следующее вычисление получает
тот же результат. Защищённая публикация Save, property plans и read permissions
не проходят через этот выход языкового запроса.

Новых CLI options, protocol fields, строк или зависимостей нет.

## Приёмка

Регрессия создаёт 2200 методов с настоящими `SelfAssign` findings и отдельный
BSL draft с Unicode/CRLF, version 37. Реальный documentSymbol завершается
успешно; реальный completion отказывает для неверной позиции. Отдельные
завершения того же общего helper проверяют отмену и истёкший deadline.
На RU/EN после каждого случая уменьшается heap native diagnostic values;
сохранены текущие inputs, editor parse hint, revision, resources и точные
диагностики при повторном вычислении. Диск не изменяется.

Контроль с прежним выходом documentSymbol без sweep падает на проверке
уменьшения diagnostic heap. Это проверка освобождения computed values;
число memo identities и снижение RSS не используются как её заменитель.

Итоговые gates и release-измерения большого CF приведены в
[машинном отчёте](analyzer-query-cache-lifecycle-results.json).

Все 17 финальных gates прошли на неизменных 2618 локальных входных файлах:
root/SDK fmt, check, strict Clippy, default/all-features tests, SDK Rustdoc
и debug/release binaries. Root — 1543/1664 passed, по 24 ignored;
SDK — 265/604 passed без ignored. Первый lint тестовой фикстуры исправлен;
финальный набор выполнен после исправления.

На одном прежнем standalone CF выполнены одинаковые 500 current field updates
и 512 completion requests для каждой локали каждой release-сборки.

| Сборка | Медиана update + completion, EN / RU | Peak observed RSS двух запусков |
|---|---:|---:|
| До общего finish | 35,44 / 35,48 ms | 94,35 MiB |
| С общим finish | 35,44 / 35,51 ms | 95,14 MiB |

Все выбранные поля, hashes, versions и close сохранены; все completion results
остаются явно incomplete. Это наблюдение задержки и RSS выбранных зависимостей,
а не доказательство снижения RSS или отсутствия утечки. Нулевая source quota,
stale refusal и точный replay в новый процесс отдельно прошли на RU/EN.

Полный IDE pull ранее проблемного saved модуля занял 14,34–14,35 s;
peak observed RSS двух запусков — около 407 MiB. Сохранились 19 findings,
включая `WrongDataPathForFormElements`, и явная неполнота моделей.
Результат на RU/EN совпал с принятым checker snapshot из
[предыдущей приёмки](analyzer-unicode-ranges.md): source/settings hashes и
code/severity/byte spans. Полный checker в этом пакете повторно не запускался.
Весь состав — 22117 файлов и 6327 каталогов — совпал с тем snapshot и сохранён.
Все десять дочерних процессов завершены и reaped; внешние guards не сработали.
Копий большого проекта и новых информационных баз нет. Финальный scanner
подтвердил четыре сохранённые базы playground и отсутствие лишних.

## Граница политики памяти

Это единый lifecycle очистки вычисленных кешей, а не общий byte limit.
[Квота исходников](source-budget.md) и document limit продолжают ограничивать
свои входы; LRU caps ограничивают окна computed values по числу entries.
Неполные [kernel estimates](kernel-memory.md) не задают общий heap/RSS.
Append-only native type arena, process-global interner, immutable composition,
внешние `Arc`, временные аллокации и allocator требуют отдельного решения.
Пересоздание одной native-базы не освобождает process-global canonical names.
EF-18 и MVP остаются в работе.

Следующий [process lifecycle](analyzer-process-retirement.md) добавляет
пороговый отзыв поколения и exact journal replay, включая освобождение
process-global names при выходе старого процесса. Итоговая приёмка EF-18
зафиксирована там; приведённые выше результаты относятся к исходному cache-срезу.
