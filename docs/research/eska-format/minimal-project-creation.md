# Создание минимальных проектов

`eska-project new` создаёт исключительно новый ESKA-проект четырёх типов:
configuration, extension, processing, report. Платформа 1С, Git и IDE для
создания не нужны. Используется тот же Queries importer, accepted base,
initial ancestor и manifest-last publication, что и при импорте Designer XML.

```sh
eska-project new demo --name demo
eska-project new report-demo --name report-demo --type report \
  --metadata-name НовыйОтчет --initial-version 2026.10.0.1 --format json
eska-project --project-dir existing-parent new child --name child --type extension
```

## Исходники и версия

По умолчанию тип — configuration, версия — `0.1.0`. Существующая модель
`ProjectVersion` проверяет `x.y.z` и `x.y.z.build`; выбранная схема записывается
до публикации manifest. CF/CFE хранят единственную редактируемую версию в YAML.
EPF/ERF — в экспортной функции `Версия()` отдельного object module; свойства
Version в метаданных внешнего объекта нет. Генерируемый BSL имеет стандартное
описание возврата, область программного интерфейса, UTF-8 BOM и CRLF.

`--name` задаёт переносимый project label. Для нативного имени по умолчанию
дефисы заменяются подчёркиваниями, перед начальной цифрой добавляется `Project_`.
Явный `--metadata-name` сохраняется точно, включая Unicode; проверяются
идентификатор и переносимое имя XML-файла. `--platform-version` только
проверяется и сохраняется. Без него platform override отсутствует и доступно
обычное наследование при последующем подключении member.

Нативный профиль Designer 2.20 содержит пустой состав и необходимые
InternalInfo. ClassId платформы фиксированы; root UUID, ObjectId, TypeId и
ValueId создаются заново. Для настоящего CFE сохраняются `ObjectBelonging =
Adopted` и `ConfigurationExtensionPurpose = Patch`; проверяется фактический тип
обратной выгрузки. Наличие одного Purpose оказалось недостаточным в прототипе.

Случайные идентификаторы получают системную энтропию через fallible
[`getrandom::fill` 0.4.3](https://docs.rs/getrandom/0.4.3/getrandom/fn.fill.html)
с минимальными features. Этот пакет уже был в трёх lockfiles; добавлена только
прямая dependency edge. Отдельная UUID-библиотека не нужна.

## Публикация и machine interface

Временный native input создаётся в исключительно захваченном sibling каталоге
существующего canonical parent. Общий importer создаёт src, accepted base,
raw initial history и transport, затем публикует `eska.toml`. Временный input
удаляется после успеха и ошибки. Ошибка очистки возвращает
`source_cleanup_failed`; уже опубликованный project root сохраняется.
Существующий destination, включая dangling symlink, не перезаписывается.

Относительный destination разрешается от `--project-dir`, абсолютный сохраняет
свой смысл. Parent должен существовать. Core не запускает платформу или Git
и не выполняет workspace enrollment: для него используется общая модель
discovery/membership. Нативные файлы внутри временного каталога не становятся
второй редактируемой копией.

Tool envelope остаётся версии 1, `command = "new"`. Data содержит ровно
`root`, `name`, `type`, `metadata_name`, `initial_version`, `version_scheme`,
`source_format`, `source_layout`, `source`, `accepted_base`,
`accepted_base_sha256`, `ancestor_sha256`, `manifest_sha256`. Source layout —
Queries. Два candidate pins требуют независимого подтверждения и не заменяют
друг друга. Manifest hash вычисляется из точной serializer строки до
публикации; последующее изменение файла не меняет возвращённый digest.
Он нужен оболочке для expected-byte финализации workflow settings.

Capabilities добавляет `new`, `new_project_types`, `new_source_layout =
"queries"` и `new_initial_version = "0.1.0"`. Старые import JSON/defaults
сохранены. Экспериментальный Rust `ImportedProject` дополнен
`manifest_sha256`; документированные старые JSON contracts не изменены.

## Проверки

Core и CLI matrices проверяют четыре типа, свежие UUID, независимые support/
ancestry, обе схемы версии, точные Unicode имена, BOM/CRLF, оба языка,
human/JSON/no-TTY, отказ неверных имён/версий и сохранение существующего output.
Проверяется исходный manifest hash после посторонней правки опубликованного
файла и разрешение destination относительно `--project-dir`.

Все 22 Rust gates прошли: root 828 unit/610 default integration (612 на
all-features), 27 build/60 project process, SDK 187/161/19, adapter 70 + 12,
minimal/build-core/build-CLI и strict Rustdoc. 24 opt-in root cases пропущены.
Ключи RU/EN совпадают; новые CLI cases проверяют exit/stdout/stderr.

Все восемь вариантов нового constructor прошли actual CLI new → shell build →
независимый native unpack на Linux/1С 8.3.27.2325: четыре типа и обе схемы версии.
Проверены фактический тип, точные Unicode Name, root UUID, InternalInfo identities
и версия. Четыре внешних модуля совпали побайтно с BOM/CRLF; saved source,
accepted/initial history и settings не изменились, owned staging удалён.
Пустым внешним профилям в этой матрице base CF не потребовался.

Четыре сгенерированных модуля прошли `bsl-analyzer-app` 0.3.2: complete,
failed files 0, diagnostics 0. Workspace MCP недоступен; в собственных demos
`bsl-analyzer.toml` отсутствует, использованы defaults. Это проверка минимального
генерируемого кода, не семантическая приёмка всех внешних объектов.
[Обезличенная матрица](minimal-project-creation-results.json).

Общая оболочка new, new from native artifacts, init, migration и приёмка MVP
продолжаются в EF-20; этот constructor не закрывает этап.
