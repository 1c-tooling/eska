# Тесты и покрытие Rust-кода

В репозитории четыре самостоятельных Cargo-проекта. Корневой `cargo test`
не запускает тесты `analysis-core`, `bsl-adapter` и `bsl-kernel`. Кроме того,
корневые тесты самостоятельных `eska-project` и `eska-build` включаются features
`project-cli` и `build-cli`; обычный запуск с default features их не включает.

## Полный набор

Команды выполняются из корня репозитория. Playground — соседний
`../eska-playground`; файловые сценарии создают собственные уникальные каталоги.
Существующие пользовательские demo не используются для автоматических проверок.

```bash
export ESKA_TEST_ROOT="$(realpath ../eska-playground)"

cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps

cargo fmt --manifest-path tools/analysis-core/Cargo.toml --check
cargo check --manifest-path tools/analysis-core/Cargo.toml --all-features --locked
cargo clippy --manifest-path tools/analysis-core/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path tools/analysis-core/Cargo.toml --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path tools/analysis-core/Cargo.toml --all-features --locked --no-deps

cargo test --manifest-path tools/bsl-adapter/Cargo.toml --locked
cargo test --manifest-path tools/bsl-kernel/Cargo.toml --workspace --locked
git diff --check
```

Это общий тестовый набор для ревью. Изменения адаптера или ядра дополнительно
требуют собственных fmt/check/Clippy/Rustdoc; изменения feature gates — проверки
соответствующих минимальных наборов features. Платформенная приёмка описана в
[проверке сборки](build-validation.md), правила завершения — в
[AGENTS.md](../AGENTS.md#проверки-и-завершение-задачи).

## Воспроизводимое измерение

Используется [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)
и [source-based coverage Rust](https://doc.rust-lang.org/rustc/instrument-coverage.html).
Для замера 2026-10-11 применялись Rust 1.98.1, LLVM 22.1.8 и
cargo-llvm-cov 0.9.1 на Linux x86_64. Инструмент не является зависимостью проекта.

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version 0.9.1 --locked
export ESKA_TEST_ROOT="$(realpath ../eska-playground)"
export CARGO_BUILD_JOBS=2
mkdir -p target/coverage

cargo llvm-cov --all-features --locked --json \
  --ignore-filename-regex '(^|/)(tests|examples)/|(^|/)[^/]*tests\.rs$' \
  --output-path target/coverage/root.json
cargo llvm-cov --manifest-path tools/analysis-core/Cargo.toml --all-features --locked --json \
  --ignore-filename-regex '(^|/)(tests|examples)/|(^|/)[^/]*tests\.rs$' \
  --output-path target/coverage/analysis.json
```

В каждом запуске собирается отдельный профиль своего Cargo-проекта. Из отчёта
исключены файлы в каталогах `tests`/`examples` и файлы с суффиксом `tests.rs`.
Встроенные `#[cfg(test)]`-модули внутри остальных файлов остаются в знаменателе.
Поэтому это покрытие инструментированных строк выбранных файлов, а не точная
доля production-кода или реализованных пользовательских сценариев. Branch
coverage в этом замере не включён. Проценты адаптера и ядра отдельно не измерялись;
их собственные тесты не входят в процент анализатора.

Сравнивать нужно одинаковые features и фильтр файлов. Перенос тестов из inline
модуля в исключённый файл меняет знаменатель; сам по себе рост процента не
доказывает улучшение проверок. Для значимых отказов нужен регрессионный сценарий,
который падает до исправления.

## Ревью 2026-10-11

Начальное состояние — `3710d41`, проверенное состояние кода — `b252e21`.
Покрытие строк до и после:

| Область | До | После |
|---|---:|---:|
| Основной проект, все features | 38 489 / 44 779 (85,95%) | 38 531 / 44 764 (86,08%) |
| Анализатор и пакетная проверка, все features | 7 042 / 7 614 (92,49%) | 7 121 / 7 629 (93,34%) |
| Публикация артефакта и манифеста | 45,09% | 75,32% |
| Жизненный цикл сервера | 81,11% | 90,50% |
| Выделенная сериализация ответов | — | 90,63% |
| Представление причин диагностики метаданных | 44,57% | 100,00% |

В модуле публикации прежний inline-тест вынесен в отдельный файл: сравнение
процентов включает изменение знаменателя. Регрессия подтверждена независимо:
невозможность восстановить артефакт прерывала восстановление манифеста.
Теперь обе операции выполняются независимо; не восстановленный backup сохраняется.
Добавлены отказы обоих шагов публикации, сочетания существующих/отсутствующих
выходов и проверка очистки staging/backup.

Сериализация отделена от жизненного цикла сервера с сохранением JSON-RPC
контракта. Тесты проверяют точный лимит UTF-8/JSON bytes, ID и snapshot при
переполнении, слишком большой ответ об ошибке и замену тела при отмене.
Настоящие stdio-процессы проверяют восстановление после неправильного JSON,
завершение при повреждении framing и отказ запросов после shutdown в RU/EN.
Локализация проверена для всех 14 причин адаптера, пяти причин parser и
неподдерживаемого вида метаданных: аргументы сохраняют исходные Unicode-значения,
severity различает неподдерживаемую возможность и ошибочный вход.

| Набор | Passed | Ignored |
|---|---:|---:|
| Основной проект, default features | 914 | 14 |
| Основной проект, все features | 932 | 14 |
| Анализатор, все features | 353 | 0 |
| BSL-адаптер | 78 | 0 |
| BSL-ядро, workspace | 2 646 | 20 |

Всего без повторного учёта default-набора — 4 009 passed, 34 ignored, 0 failed.
Добавлено 16 тестов. Для изменённых проектов прошли fmt, check, строгий Clippy,
Rustdoc с `-D warnings`, обычные тесты и повторный запуск под coverage.
Для адаптера и ядра выполнены собственные тесты; их код в этом ревью не менялся.

## Оставшиеся пробелы

- `src/cli/interactive/select.rs` и `src/cli/process_output/progress.rs`:
  в автоматическом замере 0%. Нужна отдельная PTY-проверка ввода, отмены и
  восстановления терминала; тесты перенаправленного stdout её не заменяют.
- `src/cli/commands/build/errors.rs` — 37,50%, `src/cli/diagnostics.rs` — 55,30%,
  `src/build_core/tool.rs` — 57,25%. Не покрыта существенная часть представления
  ошибок и платформенных отказов. В публикации остаются непроверенные отказы
  удаления файлов и исчерпание имён backup.
- `tools/analysis-core/src/analyzer_cli/language.rs` — 65,52%: остаются пути
  отказов и защитные проверки согласованности. Высокий общий процент их не исключает.
- 14 ignored в основном проекте требуют явных входных корпусов/ручных стендов.
  Среди 20 ignored ядра есть как scale/external-fixture проверки, так и известные
  ограничения: отслеживание колонок `ValueTable`, часть SDBL propagation,
  обращения через `ЭтотОбъект` к табличным частям и некоторые CFE-контексты.
  Отключённые тесты не считаются пройденными.

Проверки выполнялись на синтетических изолированных fixtures и процессах Linux.
Настоящая 1С, Windows/macOS, PTY и новые замеры больших пользовательских
конфигураций в этом ревью не запускались. Эти результаты не подтверждают
платформенную приёмку, поведение IDE-клиента или общий RSS budget.
