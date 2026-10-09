# Независимая библиотека нативной сборки

`build_core` доступен через feature `build`. Здесь находятся планирование,
поиск и запуск 1С, отмена, managed infobase, копирование снимка исходников,
создание CF/CFE/EPF/ERF и публикация artifact manifest. Конвейер один:
действующие команды приложения вызывают его же.

`execute.rs` согласует этапы конвейера. Проверки без записи находятся в
`execute/preflight.rs`, публикация артефакта и manifest с восстановлением — в
`execute/publication.rs`, владение временными каталогами — в `execute/workspace.rs`.
Публичные точки входа и схемы manifest сохраняются.

Сборочная библиотека не открывает Git repository, не вызывает CLI приложения
и не использует анализатор или IDE. Caller передаёт `BuildProvenance`:
имя проекта и `GitProvenance::{Available, Absent, Unavailable}`. Если Git
не проверялся, передаётся `Unavailable`; отсутствие проверки не означает
отсутствие repository. Приложение сохраняет прежнее поведение: проверяет
Git до создания временных файлов и передаёт эти сведения библиотеке.

Снимок включает точные байты исходников. Версия берётся из скопированного
native descriptor; последующее изменение рабочего файла не меняет manifest
сборки. Native version reader перенесён в `project_core::native_version`
без изменения правил: сейчас он принимает четыре компонента. Поддержка трёх
компонентов и нового источника остаётся отдельной задачей T83.

Пути прежнего Rust API приложения сохраняют свои signatures через общую
реализацию. Новый экспериментальный API `build_core` получает provenance
явно. Публичный CLI, обе локали, JSON команды build и schema version 1
artifact manifest не изменились.

## Проверки

Полный набор fmt/check/strict Clippy/test/Rustdoc/diff check прошёл.
Приложение: 302 unit и 525 integration; минимальное source core: 116 unit;
сборочная библиотека без приложения: 126 unit. В обычном прогоне пропущены
14 opt-in тестов, в каждом изолированном ядре — три. Для обеих минимальных
сборок отдельно выполнены strict Clippy и Rustdoc с `-D warnings`.

Новые проверки подтверждают прежний JSON Git provenance, сохранение BOM/CRLF,
версию и SHA-256 сохранённого снимка после изменения рабочего XML/BSL.
Ошибочный `.git` не мешает библиотеке создать manifest с переданным
`Unavailable`. Существующие integration tests проверяют настоящие Git-состояния,
замену artifact/manifest, отмену и обе локали через приложение.

Независимый временный consumer в собственном playground подключает только
`eska` с `default-features = false, features = ["build"]`. Он собрал
защищённую native копию C1 в CF через установленную 1С 8.3.27.2325.
Артефакт — 200 446 032 байта; SHA-256 совпал с manifest, версия снимка
доступна, Git provenance — `Unavailable`. Платформа успешно завершила импорт
с пятью предупреждениями о ссылках справки. Применение к БД, клиент и полный
Designer CheckConfig в этой проверке не запускались.

Граф содержит 41 normal package для библиотеки и 42 вместе с consumer;
Git engine, HTTP, clap, TTY, Fluent, Tokio и updater отсутствуют.
Consumer служит проверкой границы.
[Самостоятельный пользовательский бинарник eska-build](build-cli.md)
использует этот же конвейер и не требует consumer или оболочки.

## Оставшаяся интеграция ESKA

Добавлена отдельная точка входа для [проверенного поколения ESKA](prepared-inputs.md).
Она сравнивает скопированный input с private proof до платформы и пишет
контекст в artifact manifest schema 2; обычная сборка сохраняет schema 1.
Планирование пути само по себе не разрешает экспорт ESKA.
[Явный формат в конфигурации](project-selection.md) и самостоятельные project/build
точки входа уже используют эту границу. Машинный runner config общий с приложением;
он доступен только в app/build CLI сборках, а source/build SDK его не включает.
[Карта файлов](source-map.md) уже сохраняется
в manifest, [координаты](source-locations.md) вычисляются в project core.
[Platform callback](platform-locations.md) добавляет origins файлов проверенного
snapshot к поддержанным адресам сообщений, сохраняя raw bytes и прежние callbacks.
