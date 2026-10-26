# Производный конфиг bsl-analyzer

`eska.toml` определяет контекст приложения и политику анализатора.
`eska-project analyzer-config` создаёт обычный `bsl-analyzer.toml` в его корне.
Один root обслуживает один выбранный контекст. Полного CLI, Git, платформы 1С,
daemon и индекса BSL для подготовки не требуется.

Закреплён опубликованный **bsl-analyzer-app 0.4.0**, machine contract **3.5**.
Перед prepare/migration проверяются версия, наличие необходимых CLI-команд и
результат реального `check-config`. Другие версии отклоняются. Встроенного
ядра, форка, vendoring и зависимости от будущего `extends` нет.

## Источник настроек

В owning `eska.toml` допустима секция:

```toml
[tools.bsl-analyzer.diagnostics.parameters]
LineLength = false

[tools.bsl-analyzer.formatting]
use_tabs = true
indent_size = 1
```

Сохраняются все поля, принимаемые закреплённым executable: диагностики,
форматирование, исключения source, platform help, minimum version, code lens,
analysis/output/search/features и другие поддержанные параметры. Полная схема
не копируется в ESKA: несовместимые или неизвестные параметры отклоняет
`check-config` до публикации. Политика принадлежит корню; member с собственной
политикой отклоняется при discovery.

Нельзя переопределять `target_platform_version`, `compatibility_mode`,
`source.root`, `source.extensions`, `source.externals`, `source.configuration`.
Они вычисляются из [контекста приложения](application-context.md): actual source
paths, dependency-ordered extensions с именами/dependsOn, внешние объекты,
target и XML compatibility основной конфигурации. Пустые extension/external
списки записываются явно, отключая автообнаружение этих источников upstream.
`source.exclude` остаётся политикой. Limited standalone контекст допустим;
полноту выгрузки для анализатора дополнительно проверяет `check-config`.

Путь, зависимости и область установки каждого расширения задаются одной записью
в `workspace.members.<приложение>.extensions`: `path`, `depends_on`, `scope`.
По умолчанию `scope = "InfoBase"`; для области данных задайте `DataSeparation`.
Для отдельного расширения переопределение — `analysis.extension_scope`.
Генератор всегда записывает эффективное значение в `source.extensions[].scope`.
При миграции существующий явный `scope` должен совпадать с ESKA, иначе preview
сообщает конфликт и сохраняет исходный конфиг.
[Примеры и смысл областей](application-context.md#область-установки-расширений).

Пути рабочих участников относительны к корню итогового файла. Явная
`configuration.source` вне этого корня записывается абсолютным путём;
содержимое её корневого XML также входит в fingerprint. Каждый config/snapshot
input ограничен 64 MiB. ESKA не сканирует BSL при подготовке. Поведение самого
upstream `check-config`, включая чтение метаданных и platform-help, остаётся его
контрактом; нативный runner 1С не вызывается.

## Команды

```sh
eska-project analyzer-config status --format json
eska-project analyzer-config prepare --analyzer /path/to/bsl-analyzer-app
eska-project analyzer-config migrate --analyzer /path/to/bsl-analyzer-app --format json
eska-project analyzer-config migrate --apply --preview-token TOKEN --analyzer /path/to/bsl-analyzer-app
```

Глобальные `--project-dir`, `--lang` выбирают root и язык вывода;
`--application` внутри analyzer-config выбирает приложение. `--analyzer` можно заменить `ESKA_BSL_ANALYZER`; иначе используется
`bsl-analyzer-app` из PATH. `--timeout` задаёт секунды на subprocess, default 60.
Успех — exit 0, ошибка — exit 1, отмена — exit 130. `status` возвращает exit 0
только для `current`; он ничего не записывает и не запускает внешние процессы.

JSON schema 1 содержит `kind: "analyzer-configuration"`, `status`, `changed`,
path DTO, `ownership`, `preview_token`, `limited_context`. Ownership включает
версию генератора, версию анализатора, выбранный контекст, input fingerprint
и SHA-256 тела. Fingerprint учитывает root/member manifests, выбранные root XML
и результат генерации; BSL тексты в него не входят.
Статусы: `current`, `missing`, `stale`, `migration_required`, `managed_conflict`,
`context_conflict`. Переводы и upstream human logs не входят в JSON.

## Владение, сохранность и несколько окон

Генератор добавляет ownership header и проверяет checksum ранее созданного
тела. При ручной правке он возвращает конфликт. Для исправления сохраните
правки в первичном конфиге, сохраните изменённый derived файл отдельно и только
после этого удалите его для новой подготовки. Автоматического force overwrite нет.

Кандидат создаётся исключительно в том же корне: относительные пути при
`check-config` разрешаются так же, как после публикации. После проверки
повторно сверяются snapshots, затем выполняется rename. Неизменённый файл
не перезаписывается. Ошибка, таймаут или отмена до публикации оставляют прежний
конфиг; устаревший fingerprint запрещает использовать его для нового анализа.

OS lock `.eska/analyzer-config.lock` защищает публикацию. Работающий клиент
удерживает shared lease; несколько окон одного контекста переиспользуют файл.
Запись требует exclusive lock. Для смены приложения нужно остановить LSP
во всех использующих его окнах, освободить leases и вызвать prepare с
`--switch-context`. Другой контекст без явного switch даёт конфликт.
Клиент получает/releases lease через [Metadata API 1.12](ide-protocol.md#api-112-контекст-и-конфигурация-внешнего-анализатора).
EOF отменяет работающий validator; завершение процесса освобождает lock.

## Миграция

Пользовательский файл не становится generated автоматически. Preview показывает
полную импортируемую политику, прежние managed значения, несоответствия,
предлагаемые manifest/config и token. Проверяются оба конфига выбранным
анализатором. Совпадающие canonical paths принимаются; несовпадения target,
compatibility, путей, зависимостей или уже заданной ESKA policy требуют решения
в первичном конфиге и нового preview. Неподдерживаемая source-композиция не теряется.

Применение требует точного token, связывающего исходные байты обоих файлов
и контекст. Оригинал сохраняется как `bsl-analyzer.toml.eska-original-<hash>`;
существующая резервная копия не перезаписывается. Политика переносится в
`tools.bsl-analyzer` с сохранением остальных manifest полей/комментариев.
Обе записи выполняются под одной lock; обычная ошибка второй публикации
откатывает точные байты первой. Конкурентные внешние правки не заменяются
откатом. Аварийное завершение между двумя rename не является файловой
транзакцией: следующий запуск проверяет владение/fingerprint и отказывает в
новом анализе несогласованного состояния. Оригинал остаётся в backup.
После миграции вручную редактируется только ESKA; обратной синхронизации нет.

На отдельной reflink-копии `buhmfo` проверены preview ru/en, apply, оригинал,
шесть источников и идемпотентный prepare. Исходный проект и baseline не изменены;
[измеренные команды и fingerprints](measurements/t84-configuration.json).

Проверки T84: fmt, workspace check/strict Clippy/test/Rustdoc прошли.
Полный Rust-прогон: 782 passed, 9 ignored. Три дополнительные проверки с
опубликованным analyzer выполнены явно: четыре project kinds/unknown policy,
CLI migration и два API-клиента с shared leases. Отдельный настоящий RPC
preview побайтно совпал с JSON CLI; apply, backup и acquire/release прошли.
138 Node-тестов и Extension Host properties (VSCodium 1.109.5) прошли.
Проверены реальные ошибки второго rename, stale token, manual edit, timeout,
отмена запущенного validator и EOF; его PID не остаётся после отключения.

Историческая приёмка T84 использовала прежнюю схему workspace; её сохранённый
стенд не является примером текущего формата `designer-xml-3`.

Результаты остальных ОС и полноценного собственного LSP относятся к T89;
подготовка конфига сама по себе не подтверждает успешный анализ BSL.

## Пороги CI

`[tools.eska-check]` задаёт корневую политику одноразовой проверки:

```toml
[tools.eska-check]
max_errors = 0
max_warnings = 0
# max_information = 0
# max_hints = 0
```

По умолчанию разрешено ноль ошибок; остальные уровни не ограничены. Лимиты —
неотрицательные целые числа, неизвестные ключи отклоняются. Они сохраняются
отдельно от `[tools.bsl-analyzer]`, не попадают в upstream TOML и наследуются
вместе с составом приложения из корневого манифеста. Member не задаёт свою
политику. Настройки правил и сообщений остаются в `[tools.bsl-analyzer]`.

Адаптер `eska-analyzer::check` использует реальный JSONL-контракт 0.4.0:
точные `start/file/done`, один результат на каждый ожидаемый BSL-файл,
нулевые `failed_files`, проверенные счётчики и успешный exit процесса.
Повреждение потока, пустой набор, timeout, отмена и изменение XML/BSL/манифеста
не возвращают завершённый отчёт. Проверяется весь выбранный контекст;
`analysis.diff_base` и непустой `analysis.ignored_authors` отклоняются как
частичная политика. Литеральные `source.exclude` сохраняют значение upstream;
неоднозначные symlink и пути вне root отклоняются.

Структурная проверка обходит объявленные XML-дескрипторы общими resolver/parser
ядра, без IDE-индекса. Неподдерживаемый фрагмент тоже означает незавершённую
проверку. Она не заменяет компиляцию платформой и не проверяет непрозрачные
данные форм. JSONL читается построчно: 8 MiB на событие, 256 MiB на поток;
превышение ограничений не превращается в успешный частичный отчёт.

Промежуточная проверка адаптера T93: 180 тестов core/analyzer passed,
4 ignored; оба явных теста с опубликованным бинарником прошли отдельно.
Реальная матрица покрыла четыре standalone-типа и выбранное приложение
из базы, пяти расширений и EPF с исключением каталога и невыбранного member.
Workspace strict Clippy прошёл; финальный полный набор выполняется с CLI T93.
