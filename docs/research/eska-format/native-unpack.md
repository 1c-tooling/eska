# Самостоятельная распаковка нативных артефактов

Первый этап EF-25 выделяет распаковку CF/CFE/EPF/ERF в `build_core::artifact`.
Существующие `eska new --from` и `eska import` вызывают тот же конвейер.
Build core не выбирает проект/member, не открывает Git и не разрешает замену
рабочих исходников. Native identity читается через `project_core::native`.

## Команда

```sh
eska-build unpack input.cf xml-output --platform-version 8.3.27.2325
eska-build unpack input.cfe xml-extension --platform-version 8.3.27.2325
eska-build unpack input.epf xml-processing --platform-version 8.3.27.2325
eska-build unpack input.erf xml-report --platform-version 8.3.27.2325 --format json
```

Нужен существующий родитель нового каталога. Входной файл и destination
разрешаются относительно текущего каталога; global `--project-dir` не задаёт
контекст распаковки. `eska.toml`, принятой базы и Git не требуется.
`--ibcmd`, `--platform-arch` и `--distrobox` используют общую машинную настройку
и проверку точной версии платформы. Расширение имени файла не подменяет native
identity: тип, UUID и Name берутся из полной XML-выгрузки.

Сначала проверяется отсутствие destination, затем запускается платформа.
Входной артефакт копируется в принадлежащий операции staging под `.eska/import`
рядом с destination. CF/CFE проходят загрузку в собственную ИБ и повторный
export с проверкой identity и `ConfigDumpInfo.xml`. Частичная выгрузка не
публикуется. Распаковка сохраняет байты ресурсов, модулей и пустые каталоги;
дальнейший project import выполняет проверки проекции и поддержки.

JSON использует общий envelope schema 1, `tool = "eska-build"`,
`command = "unpack"`. `data` содержит `destination`, `project_type`, `uuid`,
`name`, `files`, `platform_version`, `runner`, `source_format = "designer-xml"`,
`project_created = false` и `platform`. Каждое native событие сохраняет
`stream` и точные `bytes`; при ошибке события находятся в
`error.diagnostics.platform`, подробности — в `error.diagnostics.detail`.
Ключи и коды не зависят от locale. Human output и пояснения доступны RU/EN.

## Владение и публикация

`PreparedArtifact` владеет staging до удаления объекта. `sources()` предоставляет
каталог для следующего core-этапа; `verify()` сверяет его с неизменным снимком
подготовки. `publish_sources(destination, cancelled)` заявляет новый каталог
через exclusive create, копирует только свои проверенные файлы и повторно
проверяет исходный и опубликованный снимки. Файлы создаются без перезаписи.
Отмена проверяется при обходе, чтении и копировании ресурсов.

Существующий destination, включая dangling symlink, вызывает отказ. Если другой
процесс создаст destination во время работы платформы, его каталог сохраняется.
Изменённые bytes, новые файлы и ссылки в staging отклоняются.
При обычной ошибке или отмене удаляется только собственный незавершённый output;
RAII очищает staging. Directory publication не является атомарной транзакцией
на случай потери питания: при аварийном завершении процесса возможен частичный
новый каталог, повторная команда его не перезаписывает. Применение к существующему
проекту с журналом восстановления остаётся следующей частью EF-25.

Экспериментальный Rust API перенесён из `project::artifact` в build core:
`PreparedArtifact::unpack` возвращает отдельный `UnpackError`. Ошибки старых
команд преобразуются в прежние коды. Это не меняет CLI/JSON Designer XML import.

## Приёмка

Семь unit-сценариев хранения и exact-byte публикации дополнены отменой после
заявки output. Шесть самостоятельных process-сценариев проверяют четыре типа,
неверный CF/CFE suffix, native отказ, обрезанный descriptor, existing/concurrent
output, SIGTERM, staging cleanup, BOM/CRLF, обе локали и стабильный JSON.
Прежние XML import/new сценарии остаются в общем наборе.

На 1С 8.3.27.2325 пройдены восемь случаев четырёх native типов в RU/EN:
13/107/8/4 файлов. Обе успешные выгрузки каждого типа совпали побайтово, включая
ConfigDumpInfo; входные артефакты сохранены. Первичный CF file export один раз
вернул `Invalid data storage format` и обрезанный descriptor. Output не создан;
отдельный запуск с теми же CF bytes в новом destination прошёл. Это повторяет
наблюдение EF-15; причина сбоя не установлена, автоматический retry не добавлен.
[Обезличенные результаты](native-unpack-results.json).

Все 22 проверки прошли: root fmt/check/strict all-targets/all-features Clippy,
default и all-features tests, Rustdoc, minimal check; SDK и adapter fmt/check/
Clippy/tests/Rustdoc; минимальный build core check/Clippy и build CLI tests/
Rustdoc. Root: 757 unit, 600 integration; all-features дополнительно 21 build
и 49 project process tests. Minimal build CLI: 592 unit и 21 process tests.
Обычный root запуск пропустил 24 opt-in случая; они не считаются подтверждёнными.
Локали содержат одинаковые 1 103 ключа, ссылки и diff проверены.
EF-25 остаётся `IN_PROGRESS`: ancestor, трёхстороннее слияние и восстановление
существующего проекта ещё не подключены к распаковке. Команда `unpack` не
принимает новую базу поддержки и не обновляет сумму CI.
