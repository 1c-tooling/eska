# Освобождение прежнего разбора недоступного BSL

В общей сессии исправлено удержание reparse snapshot после освобождения
сохранённого модуля или отказа текущего ввода. Native `Files` хранит этот hint
отдельно от Salsa: очистка её LRU не удаляла прежнее дерево и текст из hints.
Регрессионный SDK test подтвердил ошибку на исходном ядре: source-byte счётчик
уже был равен нулю, но `parse_snapshot(file)` оставался заполненным.

## Исправление

Локальный `base-db` той же закреплённой ревизии стал пятым Cargo patch.
`Files::set_file_unreadable()` удаляет hint после exclusive input write.
Это единая точка для release, missing input, отмены/ошибки чтения и отказа
по квоте; отдельного списка модулей или нового кеша нет. Файл остаётся отмечен
unreadable с пустым resident text: disk-backed fallback не включается.
Другие файлы и их hints не затрагиваются.

Готовые входы сохраняют incremental reparse. Повторная загрузка тех же bytes
может использовать корректный Salsa memo, не создавая hint заново;
полученный syntax result и current hash проверяются отдельно. Изменённый текст
разбирается заново и создаёт hint по текущим bytes. Query/transport API,
editor versions, ревизии, лимиты source bytes и существующая LRU policy
не изменились. Это освобождение одного внешнего держателя дерева, не полный GC.

Скопированы семь оригинальных Rust/manifest файлов; их исходные hashes внесены
в [provenance](../../../tools/bsl-kernel/upstream-provenance.json).
Кроме production-правки добавлен native test, исправлены три private links
Rustdoc и единственный временный fixture `base-db` перенесён в ESKA_TEST_ROOT.
Версии crates, лицензии/NOTICE, исходный checkout и publication pipeline
не менялись. Патчи явно объявлены в корнях kernel/analysis workspaces:
Cargo не наследует patch зависимости.

## Что измерено

Отдельный release probe применял обычные SDK load/diagnostics/release и
cache trim через каждые 32 модуля. Для каждого C1/C2/C3 выбраны 128 самых
крупных объявленных текстовых CommonModules. Полный composition открыт;
owner YAML остаётся pending. Профиль — только syntax, source bytes не менялись.
По одному новому процессу для исходного и исправленного ядра использовали
одинаковые selections. Всего — 768 проверок настоящих тел, по пять samples
на процесс. После каждого release current module_text_bytes равен нулю.

| Состав | Сумма выбранных BSL bytes | Финальный RSS до исправления, KiB | После, KiB | Salsa heap estimates до/после, bytes |
|---|---:|---:|---:|---:|
| C1 | 31713189 | 251572 | 245216 | 70081100 |
| C2 | 130405683 | 870504 | 816756 | 317542836 |
| C3 | 138176718 | 919548 | 854380 | 339987599 |

Это единичные парные Linux-замеры, не статистическая оценка или обещание RSS.
Allocator может сохранять освобождённые страницы. Все доступные Salsa heap
estimates совпали на одинаковых stages; удаляемые hints хранятся вне Salsa.
`entries` считает instances, включая metadata-only memos с вытесненным
значением: финальные 128 parse entries не означают 128 живых parse trees.
Нулевой aggregate также может происходить от таких memo; смешанный aggregate
может пропускать неинструментированные values. Смысл счётчиков уточнён
в [SDK отчёте](kernel-memory.md) и [IDE протоколе](analyzer-kernel-memory.md)
без изменения схемы.

Проверены bytes 390 выбранных оригиналов: 384 BSL и шесть control files;
изменений нет. PATH пустой, все шесть процессов завершились с exit 0.
Этот замер не загружает все модели метаданных и не анализирует все модули.
Общий heap/RSS budget, полная семантика и предотвращение OOM не реализованы.
Платформа 1С и настоящий IDE-клиент здесь не проверялись.

## Проверки

Два новых SDK tests проверяют release, rejected overlay, независимость hint
соседнего файла, отказ диагностики unavailable input и восстановление current
saved hash/results. Native test проверяет unread flag, пустой resident text,
освобождение только выбранного hint и последующий разбор изменённых bytes.
Остальные сценарии saved/overlay/close и RU/EN прошли существующие process tests.

All-features: 108 unit/file, 119 analyzer и 11 checker process tests, всего 238.
Minimal — 86; checker — 88 unit/file и 11 process; analyzer — 108 unit/file
и 118 process. Все 19 tool и 15 root gates прошли; root — 895/14, minimal —
165/3, project-cli — 182/3, build-cli — 201/3. Kernel workspace прошёл
fmt/check/strict Clippy/test/Rustdoc с warnings denied: 1102 passed / четыре
прежних ignored, включая 48 tests `base-db`.

Normal graph обоих инструментов — прежние 206 имён/214 версий; существующий
пакет base-db заменён локальным patch, новых package versions нет. Release
analyzer/checker — 50090416/46269152 bytes. Human strings не добавлены;
CLI/JSON schemas не изменены.

Из корня репозитория:

```bash
ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test --manifest-path tools/analysis-core/Cargo.toml --all-features --locked memory::tests
ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test --manifest-path tools/bsl-kernel/Cargo.toml --workspace --all-features --locked
```

[Обезличенные результаты](kernel-parse-release-results.json) отделяют OS RSS,
Salsa estimates и source quota. EF-18 остаётся `IN_PROGRESS`.
