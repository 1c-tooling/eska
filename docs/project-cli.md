# Самостоятельный проектный инструмент

`eska-project` работает с Designer XML без полной ESKA, Git, платформы 1С и
анализатора. Модель, правила XML и проверенные операции записи находятся в
общем ядре. Существующий `eska` сохраняет прежние команды и их поведение.

Сборка из checkout: `cargo build -p eska-project`.

Примеры внутри отдельного каталога соседнего `eska-playground`:

```sh
eska-project new demo --type configuration
eska-project new report --type report --source "Исходники XML"
eska-project init existing-export --source src
eska-project --project-dir demo validate --format json
eska-project --project-dir demo context --format json
eska-project --project-dir demo version --format json
eska-project --project-dir demo metadata inspect
```

`new` создаёт манифест и пустой source без генерации XML; требует тип.
`init` определяет тип по существующему корневому XML.
Поддерживаются configuration, extension, processing и report. `--source`
задаёт относительный каталог без `..`; `.` допустим. Существующий destination
или манифест не перезаписывается. Инициализация сохраняет исходные байты XML,
BSL и непрозрачных файлов. Git не инициализируется. В standalone создаются
обычные `.gitignore` и `.gitattributes`; workflow не выбирается.

Необязательный `--platform-version 8.3.27.2325` сохраняет точную версию сборки
как данные, без поиска или запуска платформы. Без него standalone-манифест
содержит пустую версию. В workspace версия наследуется от корня; явное значение
записывается в member как override.

При запуске `new` внутри workspace позиционный аргумент — переносимое имя
участника, который создаётся в `src/<имя>`. `init` подключает существующий
каталог, использует его имя либо `--name`. Оба пути обновляют `workspace.members`
с сохранением комментариев, проверяют границы и не создают вложенные Git/workflow
файлы. Общий writer откатывает только принадлежащие операции файлы; конкурентно
изменённый root-манифест не перезаписывается.

`validate` и запуск без команды проверяют манифесты, выбранные корневые
дескрипторы, типы и границы исходников. Они не обходят все дочерние XML и не
проверяют BSL или возможность сборки платформой. В workspace доступны
повторяемый `--project <имя>` и `--workspace`. Пустой выбор возвращает ошибку.

`context` использует [правила контекстов приложений](application-context.md),
принимает `--application` и читает только корневые дескрипторы выбранных
источников. Standalone-расширение, EPF и ERF допустимы и отмечаются
`limited_context: true`. Команда не создаёт `bsl-analyzer.toml`, кеш или индекс.

`analyzer-config` готовит [производный конфиг и миграцию](analyzer-configuration.md).
`metadata` и `version` перенесены без изменения операций, accepted-base checks,
ownership снимков и существующих JSON-схем.

`serve --stdio` запускает самостоятельный сервер [Metadata API](ide-protocol.md).
Он объявляет identity `eska-project`, API 1.13 и `selfUpdate: false`; языковой
сервер не запускается при открытии дерева. Полная ESKA, Git, платформа и analyzer
в PATH не требуются. Прежний `eska ide --stdio` сохраняет identity `eska` и прежнее
поведение CLI. Это два входа одной реализации сервера, без второго дерева или индекса.

## Диагностика окружения

`eska-project doctor` сохраняет отчёт общего `eska doctor`, включая `--project`,
`--workspace`, platform flags и `--format human|json`. Для проверок платформы
и VCS вызываются их отдельные компоненты; отсутствие одного отражается
ошибкой в отчёте и не скрывает остальные проверки. Обычные команды проекта
и `serve` этих компонентов не требуют. [Композиция и проверки](component-delegation.md).

## Вывод

Все команды поддерживают глобальные `--lang ru|en` и `--project-dir`.
Справка доступна без внешних процессов. `new`, `init`, `validate` и `context`
принимают `--format human|json`; по умолчанию вывод human. Успех — exit 0,
ошибка — exit 1. Ошибка разбора аргументов clap — exit 2.

JSON новых команд содержит `schema_version: 1`, `kind` и `status`. Пути
кодируются как `{value, encoding}` по общему обратимому протоколу Metadata API.
Успех создания/инициализации содержит `project` с `root`, `source`, `project_type`
и `vcs_initialized: false` — операция не инициализировала repository.
Проверка возвращает `validation: "root_descriptors"` и `projects`.
Контекст возвращает `context` с именем, корнем, target version, limited flag,
источниками в порядке зависимостей и явными `extension_dependencies`.

Ошибка новых команд выдаёт JSON только на stdout:
`{schema_version:1,kind:"project",status:"error",error:{code,data}}`.
Локализованная причина выводится на stderr. JSON не зависит от языка.
Существующие `metadata` и `version` сохраняют собственные envelopes.

## Проверка T91

Пройден полный набор fmt/check/strict Clippy/test/Rustdoc для workspace:
769 Rust-тестов passed, 6 ignored. Новые process-тесты запускают компонент
с пустым PATH: все четыре типа, `source = "."` и вложенный source,
workspace enrollment, обе локали, отказ до записи, symlink escape и
сохранность BOM/CRLF/opaque bytes. Ручное сравнение metadata/version
полного и прямого CLI дало побайтно одинаковый JSON для ru/en.
`cargo run -- --version` сохраняет выбор прежнего binary через default-run.

Explorer: 138 Node-тестов с настоящим backend и изолированный Extension Host
properties suite (VSCodium 1.109.5) прошли. Это проверка перенесённого API через
прежний `eska ide`; переход на standalone serve выполняется в T92.
Первый промежуточный Rust-прогон получил `Shelf(Locked)` в существующем
switch-тесте; полный финальный прогон прошёл без изменения этого сценария.

Нормальный граф project package не содержит полной `eska`, repository `gix`,
platform runner, Tokio, HTTP/updater или ядра анализатора. `gix-path` и
`gix-validate` остаются переносимыми утилитами модели.
Экспериментальные Rust-типы native ошибок new/init теперь содержат
`Operation(CreationFailure/InitFailure)`; публичный CLI и JSON не изменены.

## Проверка T92

`eska-project serve --stdio` и новый Explorer проверены на Linux x86_64.
Полный Rust-набор: fmt/check/strict Clippy/Rustdoc, 784 теста passed, 9 ignored.
Process-тест без PATH проверяет четыре типа, обе локали, identity/capabilities,
контекст, сохранность исходников и завершение процесса. Прежний вход
`eska ide` сохраняет свои тесты и совместимость API.

Explorer: typecheck и 143 Node-теста с настоящим project и прежним API 1.11
прошли. В VSCodium 1.109.5 прошли отдельные нативные профили общего дерева,
свойств, явного legacy и пользовательского пути с отклонением старого CLI.
Тест дерева исправлен под существующее свёрнутое состояние группы Modules;
продуктовое значение не менялось. Windows/macOS и Remote этими запусками
не проверены. Установщик project и языковой клиент остаются следующими этапами.
