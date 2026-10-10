# Пакетная очистка вычисленных BSL-текстов

Исправлено удержание source-text memos при `trim_batch_caches()`. Общий
checker уже освобождает каждый сохранённый BSL после syntax query и вызывает
deep LRU trim через каждые 32 тела. Эта очистка сокращала окно parse до 64,
но оставляла `file_text_query` с interactive cap 512. Вычисленная копия текста
продолжала жить после освобождения текущего input и native reparse hint.

В новом парном замере прежнее поведение удержало все 128 прочитанных текстов:
31,7/130,4/138,2 MB в C1/C2/C3. Это отдельный держатель внутри Salsa,
в отличие от [исправленного reparse hint](kernel-parse-release.md).

## Изменение

В существующем `base-db::set_parse_lru_sweep_mode()` окна parse и source-text
memos теперь меняются вместе: sweep cap 64, interactive cap 512. Общий
`RootDatabaseImpl::enforce_lru_deep()` выполняет прежние shrink/evict/restore;
SDK и checker используют этот же метод. Обычный interactive cap и остальные
LRU caps сохранены. Нового кеша, обхода исходников, зависимостей или настройки нет.

Очистка удаляет вычисленные значения, сохраняя текущий resident input.
Повторный запрос получает его актуальные bytes; недоступный файл остаётся
недоступным. SDK revision, source quota, editor authority, query results,
CLI/JSON schemas и лицензии не меняются. Существующие Salsa memo metadata могут
оставаться после eviction; `entries` не является числом живых текстов.

Регрессионный native test не выводит освобождение из `entries` или RSS.
Он проверяет сильное владение вычисленным `Arc<str>` через
[Weak](https://doc.rust-lang.org/std/sync/struct.Weak.html), восстановление
interactive window и повторное вычисление из сохранённого resident input
без зарегистрированного disk path. На прежнем коде test падал: ни одна из
128 source-memo ссылок не теряла сильное владение после deep trim.
Weak используется только в тесте; его backing allocation не считается
освобождённой памятью процесса, пока сам Weak существует.

## Настоящие исходники

Release probe использовал публичные SDK load/diagnostics/release/trim APIs,
полный открытый состав и 128 самых крупных объявленных текстовых CommonModules
каждого C1/C2/C3. Все owner YAML оставались pending; проверялся только syntax.
Исходный и исправленный варианты запускались отдельными процессами с одинаковой
selection и пустым PATH. После каждого release current module_text_bytes = 0;
пять samples на процесс снимались при открытии и после 32/64/96/128 проверок.

| Состав | Source-memo heap estimate до, bytes | После, bytes | Финальный OS RSS до/после, KiB |
|---|---:|---:|---:|
| C1 | 31713189 | 6210849 | 245112 / 244252 |
| C2 | 130405683 | 36124609 | 816916 / 805752 |
| C3 | 138176718 | 40089440 | 854444 / 836116 |

На каждом stage новая оценка source-memo bytes равна сумме последних
64 выбранных тел либо всех тел, если их ещё меньше. Все остальные native
таблицы совпали между вариантами; разница их суммарных heap estimates равна
разнице source-memo bytes. Финальные `entries` по-прежнему равны 128:
вытеснение values не удаляет всю memo metadata.

Все 384 пары syntax results и source hashes совпали. Первый модуль каждого
процесса дополнительно перечитан после последнего trim; syntax/hash сохранены.
Всего выполнены 768 проверок тел, шесть повторных загрузок и 30 samples.
Байты 390 выбранных оригиналов до/после сохранены. Собственный временный
probe project удалён; полные private traces остаются в playground.

Это единичные парные Linux-замеры. RSS уменьшился значительно меньше, чем
source estimates: allocator может сохранять страницы, а размеры Salsa
не охватывают весь heap и могут повторно учитывать shared objects. Это не
общий бюджет памяти и не гарантия предотвращения OOM. Полная семантика,
все модели, все BSL, настоящий IDE-клиент и платформа 1С здесь не проверялись.

## Воспроизведение

Все 19 tool и 15 root gates прошли. All-features — прежние 238 tests:
108 unit/file, 119 analyzer process и 11 checker process. Minimal — 86;
checker — 88/11; analyzer — 108/118. Kernel workspace прошёл
fmt/check/strict Clippy/test/Rustdoc с warnings denied: 1103 passed и четыре
прежних ignored, включая 49 tests base-db. Root — 895/14; minimal — 165/3,
project-cli — 182/3, build-cli — 201/3; соответствующие проверки прошли.
Normal graph обоих инструментов сохранён: 206 имён/214 версий. Human strings
не добавлены; новые dependencies и изменения manifest/lock не понадобились.

Из корня репозитория:

```bash
ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test --manifest-path tools/bsl-kernel/Cargo.toml -p base-db --all-features --locked sweep_trims_source_memos_and_restores_interactive_window
ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test --manifest-path tools/analysis-core/Cargo.toml --all-features --locked
```

[Обезличенные результаты](kernel-source-sweep-results.json) сохраняют
source-memo estimates отдельно от source quota и OS RSS. EF-18 остаётся
`IN_PROGRESS`.
