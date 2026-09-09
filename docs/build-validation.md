# Проверка сборки на поддерживаемых ОС

Документ разделяет проверку переносимого process pipeline и приёмку с настоящей
платформой 1С. Тестовый `ibcmd` подтверждает передачу нативных аргументов,
stdout/stderr, exit codes и очистку ресурсов, но не совместимость Designer XML с
конкретной версией 1С.

## Переносимый процессный стенд

`tests/fixtures/ibcmd` — отдельный Rust-процесс без shell-команд. Интеграционный
набор собирает его один раз под текущую host-платформу и передаёт путь через
публичный `build --ibcmd`.

Linux и macOS:

```bash
ESKA_TEST_ROOT="$(realpath ../eska-playground)" \
  cargo test --test integration cli::build
```

Windows PowerShell:

```powershell
$env:ESKA_TEST_ROOT = (Resolve-Path ..\eska-playground).Path
cargo test --test integration cli::build
```

Набор использует пути с пробелом и кириллицей и проверяет:

| Контракт | Сценарий |
|---|---|
| `.cf`, `.cfe`, `.epf`, `.erf` и JSON | `builds_all_native_artifact_types_with_locale_independent_json` |
| точное совпадение версии платформы | `exact_platform_version_is_required` |
| workspace, selectors и aggregate JSON | `workspace_build_uses_shared_outputs_and_distinct_json_shapes` |
| dry-run и отсутствие побочных эффектов | `dry_run_human_is_localized_and_does_not_change_the_filesystem` |
| паспорт и зафиксированный снимок исходников | `manifested_build_uses_the_captured_source_snapshot` |
| безопасная замена и очистка после ошибки | `failed_build_preserves_existing_artifact_and_cleans_workspace` |
| прерывание и очистка временной ИБ | `interrupted_build_cleans_all_owned_paths` |

Сценарий прерывания пока посылает POSIX `SIGTERM`, поэтому исполняется только на
Unix. Проверка отказа и cleanup переносима, но не заменяет проверку Ctrl+C на
Windows. Проверка символических ссылок также остаётся Unix-специфичной.

### Зафиксированные результаты

| Дата | Host | Rust | Переносимый стенд | Настоящая 1С |
|---|---|---|---|---|
| 2026-09-09 | Fedora Linux 44, x86_64 | 1.98.1 | `PASS`: 25/25, включая SIGTERM и symlink | `UNVERIFIED`: `ibcmd` отсутствует |
| — | Windows, x86_64 | — | `UNVERIFIED`: host-runner недоступен | `UNVERIFIED` |
| — | macOS, x86_64/aarch64 | — | `UNVERIFIED`: host-runner недоступен | `UNVERIFIED` |

Компиляция для другого target не меняет `UNVERIFIED` на `PASS`: нужен фактический
запуск тестового executable на соответствующей ОС.

## Протокол с настоящей платформой 1С

Для каждой проверенной комбинации сохранить:

- ОС, архитектуру, версию Rust и полный вывод `ibcmd --version`;
- точные команды `eska`, runner (`host` или `distrobox`) и пути проектов;
- exit code, stdout и stderr каждой команды;
- размеры и SHA-256 файлов `.cf`, `.cfe`, `.epf`, `.erf` и их паспортов;
- результат повторной сборки поверх существующего артефакта;
- результат Ctrl+C во время import: прежний артефакт сохранён, временная ИБ и
  staging-файлы удалены;
- дату проверки и имя исполнителя.

Минимальная последовательность для каждого из четырёх типов проекта:

```text
ibcmd --version
eska build --dry-run --format json --ibcmd <полный-путь-к-ibcmd>
eska build --manifest --format json --ibcmd <полный-путь-к-ibcmd>
```

Для workspace выполнить те же две команды из корня без selector, затем с `-p`
для одного member. На Linux сценарий Distrobox записывается отдельно с
`--distrobox <container>`; он не считается проверкой host runner. Если ОС,
архитектура или версия 1С недоступны, строка результата должна оставаться
`UNVERIFIED` с причиной.
