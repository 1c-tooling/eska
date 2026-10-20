# Самостоятельный нативный инструмент

`eska-build` собирает CF/CFE/EPF/ERF и распаковывает их в Designer XML.
Сборка из checkout: `cargo build -p eska-build`. Нужна установленная платформа
1С с точной версией из эффективных build-настроек; полная `eska`, Git,
проектный сервер и анализатор для запуска не требуются.

```sh
eska-build --project-dir "$PROJECT_ROOT" build --manifest
eska-build --project-dir "$PROJECT_ROOT" build --dry-run --format json
eska-build --project-dir "$PROJECT_ROOT" clean
eska-build unpack "$ARTIFACT" "$NEW_XML_DIRECTORY" --platform-version "$PLATFORM_VERSION" --format json
eska-build platform list --format json
```

Переменные в примере задаёт вызывающая среда. Запуск без подкоманды выполняет
обычный build. `--lang ru|en`, `--project-dir`, `--help`, `--version` доступны
на уровне инструмента. Справка не ищет проект и не запускает платформу.
До выборочной поставки T97 бинарник запускается из локальной сборки.

## Сборка и обслуживание

`build`, `clean`, `platform list`, `config init/edit` перенесены из общего CLI
без второго runner или реализации XML. Сохраняются workspace selectors,
aggregate JSON, `--output`, базовый CF для EPF/ERF, `--recreate-infobase`,
`--dry-run`, `--manifest`, точная версия и разовый выбор платформы.
Параметры `--ibcmd`, `--platform-arch`, `--distrobox`, переменные `ESKA_*`
и machine config имеют прежний приоритет. `config` изменяет только явно
выбранный пользовательский machine config; обычная сборка его не создаёт.

Общий `eska-build-runtime` отвечает за планы, аргументы, host/Distrobox,
изолированные infobases, снимки, публикацию пары artifact/manifest и cleanup.
Терминальные меню и progress находятся в общем `eska-terminal`; project/check
этот package не используют. Core по-прежнему не зависит от TTY или runtime.

Паспорт использует прежнюю schema 1 и неизменяемый снимок source.
Самостоятельный инструмент не открывает repository и записывает
`source.git.status = "unavailable"`. Общая команда передаёт путь проверенного VCS-компонента; сборщик получает
его данные через внутренний process-протокол и сохраняет прежние
available/absent/unavailable. Ошибка процесса/протокола останавливает публикацию
паспорта, а не превращается в неизвестное состояние repository.
Значение unavailable не означает доказанное отсутствие Git. Правила проверки
артефактов и source snapshot одинаковы у обоих входов.

`clean` удаляет только управляемую служебную ИБ в выбранной области,
проверяет ownership и сохраняет опубликованные артефакты и паспорта.
Повторная совместимая сборка переиспользует infobase. Прерванная сборка
сохраняет прежний результат и managed infobase, удаляет свой staging.
[Контракты и OS-матрица сборки](build-validation.md).

## Распаковка

`unpack INPUT DESTINATION` не требует проекта. Относительные пути вычисляются
от `--project-dir` (по умолчанию cwd). DESTINATION должен быть новым каталогом
с существующим родителем. Проверка этого условия выполняется до запуска
платформы; существующий файл или даже пустой каталог не заменяется.
Подтверждение или `--force` здесь не отключают границу нового каталога.

Вход копируется в принадлежащий операции staging. CF сначала загружается
в отдельную временную ИБ, CFE/EPF/ERF экспортируются платформой через `--file`.
ESKA проверяет фактические XML identity/type, symlink и состав экспорта.
Публикация использует общий core writer с эксклюзивным созданием каталога.
Отмена или ошибка убирают только созданный этой операцией output; ошибка
cleanup сообщает путь сохранённого частичного каталога. Нативный вход
сохраняется. Переименование XML и payload по предположению не выполняется.

В DESTINATION находятся только экспортированные исходники: `eska.toml`
и Git не создаются. Родительский `.eska/import/.gitignore` и управляемое
служебное хранилище переиспользуются. Для подключения экспорта далее
используется `eska-project init`. Для замены существующего source есть
`eska-build import FILE` (общий вход — `eska import FILE`) с VCS preflight и подтверждением.
Обратимая транзакция повторно проверяет исходники и changed paths перед записью.
Для `import` требуется `eska-vcs`; выбор следует общим правилам
[поиска компонентов](component-delegation.md). Флаги `--dry-run`, `--force`,
`-p/--project`, `--format human|json` и platform flags сохранены. JSON schema 1,
exit codes и порядок preview/подтверждения совпадают с общим входом.

JSON schema 1 успеха: `kind: "unpack"`, `status: "unpacked"`,
`destination: {value,encoding}`, `identity: {project_type,name,uuid}` и
`platform_version`. Identity берётся из содержимого, а не имени бинарного файла.
Ошибка: `kind: "unpack"`, `status: "error"`, `error: {code,data}`.
JSON не зависит от locale, объяснения и native stdout/stderr идут на stderr.
Exit 0 — успех, 1 — отказ, 2 — ошибка аргументов, 130 — отмена unpack.
`build` сохраняет прежний exit 1 и код `interrupted` при отмене.

## Нативный стенд

Полный повтор на Linux/POSIX Distrobox, только в новом соседнем playground:

```sh
cargo build -p eska-build --locked
ESKA_TEST_ROOT="$(realpath ../eska-playground)" python3 tests/native_build.py \
  --distrobox "$ESKA_DISTROBOX" --ibcmd "$ESKA_IBCMD" \
  --platform-version "$PLATFORM_VERSION"
```

Путь `--ibcmd` здесь относится к выбранному контейнеру. Значения задаёт
оператор для установленной платформы. Скрипт создаёт собственную маленькую ИБ,
получает XML конфигурации и расширения настоящей платформой, создаёт внешние
контейнеры с новыми UUID и проверяет их той же платформой. Каталог каждого
запуска сохраняется с owner marker, raw stdout/stderr и `result.json`.
Существующие demo и рабочие базы не меняются. Ошибка оставляет complete=false.
Скрипт сейчас требует POSIX/Distrobox; это не проверка Windows/macOS/host runner.

Структура InternalInfo внешних fixtures сопоставлена с
[XML обработки](https://github.com/lintest/TesseractOCR1C/blob/master/Example.xml)
и [описанием генератора отчёта](https://github.com/Nikolay-Shirokov/cc-1c-skills/blob/main/docs/1c-erf-spec.md).
Совместимость конкретных fixtures подтверждается фактическим build/export 1С,
а не только этими описаниями. В product нет генератора XML внешних объектов.

## Проверка T94

Полный workspace fmt/check/strict Clippy/test/Rustdoc прошёл: 805 Rust-тестов
passed, 12 ignored. Прежние CLI-сценарии сохранены; новые process-тесты
проверяют standalone без других ESKA-компонентов в PATH, все типы, обе локали,
help, паспорта, clean и отказ до запуска платформы при существующем output.
Writer отдельно проверен на отмене после первого перемещения файла.

[Нативный отчёт](measurements/t94-native.json): Linux x86_64, Distrobox
`1c-ubuntu-env`, настоящая 8.3.27.2325. Для всех четырёх типов прошли
build с паспортом, повторное использование ИБ, unpack, совпадение исходных
имени/UUID, checksum, стабильность snapshot и clean с сохранением результатов
и исходных байтов. Реальная отмена unpack завершила owned PID и удалила
свой output/staging. Отмена build во время import сохранила прежние CF,
паспорт, source и managed infobase, освободив PID и временный workspace.

Нормальные зависимости build не содержат полной ESKA, project-сервера,
анализатора или `gix`. Project/check проверены без build runtime; TTY project
использует только для интерактивного compatibility-onboarding, check обходится без него.
Windows/macOS, настоящий host runner, runtime/client-функции 1С и нативная
сборка большого `buhmfo` этим результатом не покрываются.

[Делегирование T96 и нативное сравнение](component-delegation.md) проверены
отдельно; T94-отчёт выше сохраняет исходный результат своего этапа.
