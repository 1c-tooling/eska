# Выбор источников в проекте, EF-10

Общий parser `ProjectConfig` принимает явный экспериментальный формат ESKA:

```toml
[project]
name = "sample"
type = "configuration"
source = "src"
source_format = "eska"

[eska]
accepted_base = ".eska/accepted"

[build]
platform_version = "8.3.27.2325"
```

Без `source_format` сохраняется Designer XML и прежняя сериализация TOML.
`[eska].accepted_base` обязателен для ESKA; для Designer XML раздел `[eska]`
отклоняется. Профиль пока ограничен configuration/Designer 2.20. Extension,
processing и report требуют EF-14 и не включаются по одному изменению настройки.
Неизвестные поля, включая `sha256` в `[eska]`, отклоняются.

Путь базы — непустой portable relative path относительно корня конкретного
проекта или workspace member. Alias, `..`, absolute paths, обратный separator
и reserved device names не принимаются. Это местоположение bytes, а не
подтверждение политики. Expected SHA-256 передаётся вызывающим инструментом
отдельно; для CI он приходит из защищённой настройки job. Ни YAML, ни
произвольно редактируемый project manifest не назначают сами себе разрешения.
Исходники и база не могут пересекаться или быть вложены друг в друга.
Для ESKA `source = "."` не подходит: принятая база находится в проекте,
но остаётся вне editable source inventory. Designer XML сохраняет поддержку `.`.

Discovery и selection используют один parser для обоих форматов. Workspace
сохраняет путь базы каждого member и наследует прежние build/workflow settings.
Выбор одного member не открывает YAML других member. Тип источника не выводится
из наличия `Configuration.xml` или `configuration.yaml`.

`project_core::eska_source::EskaSource` открывает выбранный проект и его scope.
Reader читает только source manifest/configuration, owner snapshots доступны
по запросу и через тот же parser overlays. Для просмотра база/платформа/Git
не нужны. Ошибка YAML остаётся ошибкой даже при лежащем рядом старом XML.

`EskaSource::prepare` открывает указанную базу с independently supplied pin
и вызывает общий защищённый exporter по текущим сохранённым входам. Результат —
private `PreparedSource`, полный `ProtectedFailure` сохраняет exclusions при
строгом отказе. Удаление производных поколений не меняет выбранную базу:
материализация повторяется по тому же pin либо завершается ошибкой.

BuildPlan сохраняет настроенный format при подстановке другого source path.
Обычные entry points сборки отвергают ESKA до временных файлов, ИБ и платформы.
ESKA требует prepared entry point и проверку реального скопированного snapshot.
Version в artifact manifest читается из материализованного native snapshot;
ESKA project config не превращает текущий YAML в XML для native version reader.

Прежние XML-only команды приложения получают явный отказ при выборе ESKA:
native build, import из CF, version/bump, patch, metadata/IDE opening и XML
object/semantic readers. Они не используют оставшийся XML. Human ошибки
локализованы в ru/en; добавленные machine codes не зависят от locale.
Самостоятельные project/build CLI и делегирование оболочки ещё не подключены.

База содержит [byte-preserved blobs](ef-09-results.md). До помещения в Git
её `blobs/**` требуют `-text -filter`; стандартное `* text=auto` изменяет
байты некоторых BSL. Protected export отвергает изменённые блоки по pin/hash.
Общий [transport plan](git-transport.md) готовит scoped attributes для source/base
с exact-byte preview и публикацией. Project CLI подключит его при создании
проекта; библиотека не запускает Git. Эффективную policy проверит VCS-инструмент.

В экспериментальном Rust API `SourceFormat::Eska` несёт проверенный
`accepted_base`, enum больше не Copy, getter возвращает reference.
Это явное изменение внутреннего SDK, без compatibility layer; документированный
CLI Designer XML продолжает использовать прежние успешные contracts.

Проверки: полный набор fmt/check/strict Clippy/test/Rustdoc и diff check,
отдельные source/build сборки. Проверены строгая конфигурация, builders,
смешанный workspace, inherited settings, scope, missing/untrusted base,
recreation поколений, bad YAML без XML fallback и отказы XML-only операций.
CLI tests проверяют exit codes, stdout/stderr и одинаковый JSON для ru/en.
Итог: 332 unit и 529 integration, 138 unit в source core и 154 в build core;
14 opt-in тестов пропущены в приложении и три в каждом минимальном ядре.
Минимальные normal dependency graphs сохраняют 37/41 packages без приложения,
Git engine, HTTP, clap, TTY и IDE runtime.

Независимый build consumer прочитал реальную C1 через новый manifest/discovery
и `EskaSource`: 2 645 owners доступны ленивому reader. Protected prepare получил
pin отдельно от TOML. Установленная 1С 8.3.27.2325 собрала CF из 200 418 448 bytes;
SHA-256 совпал с manifest. Source context/map/snapshot, версия 1.0.2.01 и
platform совпали с предыдущей принятой сборкой, все пять предупреждений
получили правильные origins. Исходники и база побайтово не изменились.
Проверка не включает применение к БД, клиента или полный Designer CheckConfig.
