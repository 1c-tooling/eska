# Unicode-диапазоны metadata-диагностик

Полный semantic checker большого standalone CF завершался с exit 3:
`analysis_diagnostic_range_invalid` для `WrongDataPathForFormElements`.
При BOM, двух CRLF и `#Область` byte 9 попадает внутрь кириллической буквы.
Native правило создавало диапазон первых девяти **байтов**; SDK правильно
отказывал до преобразования координат. [Результаты](analyzer-unicode-ranges-results.json).

## Исправление

Общий `DiagnosticsContext::file_header_range` строит диапазон первых N Unicode
scalar values через [`str::chars`](https://doc.rust-lang.org/std/primitive.str.html#method.chars).
Исправлены 11 небезопасных мест в восьми обработчиках; уже безопасный
`ordinary_app_support` использует тот же helper. ASCII сохраняет прежний
диапазон, короткий текст заканчивается на своей границе, пустой получает `0..0`.
В четырёх циклах диапазон вычисляется один раз: полный BSL не копируется
для каждого finding. Настоящие AST/method ranges сохраняются.

Правило по-прежнему привязывает metadata finding к началу BSL-модуля.
Коды, severity и findings не отфильтровываются. SDK продолжает отказывать
для split UTF-8 и координат за EOF; исправлено создание native диапазона.

Для этого существующая зависимость `ide-diagnostics` включена в локальный
workspace из прежней pinned revision. Сохранены все 651 исходный файл,
production descriptions/dictionaries, лицензия и notices. Original hashes
добавлены в [provenance](../../../tools/bsl-kernel/upstream-provenance.json).
Версии и сторонние packages не обновлялись. Тестовые конструкторы приведены
к уже существующим полям метаданных ESKA; шесть snapshot endpoints учитывают
14 Unicode scalars вместо прежних 14 байтов.

## Проверки

Native regression использует настоящее правило формы и точные raw bytes,
включая empty/ASCII/кириллицу/emoji/BOM/CRLF. Отдельная
[SDK-регрессия](../../../tools/analysis-core/tests/form_queries/header_ranges.rs)
проверяет реальные IDE/checker processes, два заголовка на RU/EN:
один finding, hash исходника, settings hash, byte endpoints и независимо
вычисленные UTF-16/scalar coordinates. Guard test сохраняет отказ для
действительно неверных координат.

Все 21 финальные gates прошли на неизменных 2617 входных файлах:
root/SDK fmt, check, strict Clippy, default/all-features tests, SDK Rustdoc,
native fmt/check/strict Clippy/tests и debug/release binaries.

| Набор | Passed | Ignored | Failed |
|---|---:|---:|---:|
| Root default / all-features | 1543 / 1664 | 24 / 24 | 0 |
| SDK default / all-features | 265 / 603 | 0 / 0 | 0 |
| Native diagnostics | 2204 | 7 | 0 |

Первоначальные native test failures сохранены: отсутствующие поля fixtures
и старые byte-based snapshot endpoints. Финальный набор выполнен после их правки.
Неизменённые upstream Markdown descriptions сохраняют свои исходные пробелы;
authored diff проверен отдельно, весь исходный набор сверяется по provenance.

## Большой CF на исправленной сборке

Linux x86_64, Rust 1.98.1, source core 0.14.2, SDK 0.0.0. Использован прежний
проект без копирования, записи исходников или запуска 1С. Проверены 3617 owners
и 1992 BSL modules; все 22117 файлов сохранили bytes/modes/mtimes.

IDE pull выбранного ранее проблемного модуля занял 14.3–14.4 s на RU/EN,
peak observed RSS — около 407 MiB. Получены 19 findings, включая один прежний
`WrongDataPathForFormElements`. Полный checker обработал все 1992 modules
за 63.8–64.0 s, peak observed RSS — около 1 GiB. Все четыре children штатно
завершились; внешние guards не сработали. Findings выбранного модуля,
source/settings hashes совпали с IDE. Весь machine JSON checker одинаков на RU/EN.

Checker возвращает `incomplete`, exit 2: `metadata_models_complete` и
`semantic_profile_complete` остаются false; unchecked modules нет.
Это завершённый прогон с явной неполнотой моделей, а не полная семантическая
проверка конфигурации. Source reserve — 242932762 из 268435456 bytes.

Сохранён и предшествующий runtime baseline: 500 YAML updates на RU/EN
с медианой около 35 ms и peak RSS около 92 MiB, quota refusal/replay и отдельный
500-cycle type/comment control. Эти измерения относятся к binaries **до**
исправления диагностик. Короткий batch с явным deadline 20 s был неполным;
он не подменяет финальный default-300 прогон.

Source quota, native LRU trim и cooperative deadline не являются общим
heap/RSS/OOM budget. Внешние guards probe — только защита измерения.
Memory policy EF-18, EF-27 consumers, полная приёмка Explorer, установка
и совместная матрица EF-16/MVP продолжаются. Большой настроенный CF/CFE
и physical UI этим standalone CF прогоном не приняты.
