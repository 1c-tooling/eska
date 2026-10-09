# Общая граница проекта: шаги EF-10

`project_core` содержит общую модель проекта, строгие project/workspace manifests,
discovery/selection, обратимые source documents, строгий YAML, native XML,
принятую базу, защиту экспорта и точечную правку. Из прежних модулей сюда
перенесены общий реестр MetadataKind, ProjectType, parser нативной поддержки,
проверка пределов XML и writer с проверкой ожидаемых байтов. Реализация у каждого
одна: действующие XML-операции используют те же типы и границы записи.

Default feature `legacy` сохраняет все действующие команды приложения.
Минимальная библиотека собирается с `--no-default-features --lib`;
этот граф содержит 37 normal packages, без gix, HTTP transport, Tokio, clap,
TTY, Fluent, self-updater и zip. Список зависимостей проверен через `cargo tree`.
Общие source operations не вызывают Git или платформу; им не передаются LSP,
VS Code и состояние IDE. Optional dependencies не меняют их domain API.

Границы задаются [Cargo features](https://doc.rust-lang.org/cargo/reference/features.html)
при компиляции. Default application API и его зависимости остаются включены
в обычной установке; интеграционный тест и прежний бинарник требуют `legacy`.

Это граница компиляции библиотеки, а не разделение флагами запуска одного
процесса. Самостоятельные `eska-project` и `eska-build` доступны через отдельные
Cargo features; выборочная установка готового комплекта ещё не реализована.
Обычная сборка и установка `eska` сохраняют прежний бинарник; версия crate
и publication pipeline не менялись. Новые файлы включаются в существующий
Cargo package, это отдельно проверено через `cargo package --list`.

Прежние project modules остаются потребителями общей реализации. Импорты типов
и отображение ошибок writer не создают второй parser или альтернативные правила.
Успешные CLI/JSON contracts Designer XML сохранены. Конфигурация принимает
[явный выбор ESKA](project-selection.md); XML-only операции дают явный отказ
для этого формата. Экспериментальные Rust types теперь
также доступны через `project_core`; это не новый стабильный SDK.

Конфигурация и discovery используют существующие правила без второго parser.
Проверка имён веток вызывает тот же `gix-validate 0.11.4`, уже закреплённый
через gix в Cargo.lock. Теперь это прямая минимальная dependency без features;
она проверяет имя и не открывает repository, не читает refs и не запускает Git.
TOML parser также общий, machine-local настройки остаются в приложении.

[Ленивый reader](source-reader.md) открывает manifest и configuration,
читает другие владельцы по запросу и связывает source locations с точными
байтами YAML, включая несохранённый текст. Публикационная проверка остаётся
полной и не подменяется успешным открытием дерева.

[Metadata model и pure XML parser](semantic-projection.md) перенесены в core;
XML и YAML выдают одни типы объектов/свойств/ссылок. Disk resolver и IDE state
остаются у своих потребителей, source coordinates относятся к своему формату.

`build_core`, включаемый feature `build`, содержит общий BuildPlan, поиск и
запуск платформы, обработку прерывания и владение временной информационной
базой. Прежний сборщик использует эти же реализации. Отдельная сборка
`--no-default-features --features build` содержит 41 normal package: к source
core добавляется обработчик сигналов `ctrlc` и его системные зависимости.
Git engine, HTTP, CLI, TTY и IDE runtime не подключаются.
[Конвейер сборки](build-boundary.md), снимок и создание manifest также перенесены.
Git проверяет вызывающее приложение; библиотека получает готовый provenance.
Native version reader теперь общий и сохраняет существующий контракт.

В экспериментальном Rust API доступны lifecycle ManagedInfobase, запуск
Designer и замена source path в BuildPlan. Планирование пути не проверяет
принятую базу и не разрешает экспорт ESKA: эту проверку выполняет защищённая
материализация. Публичные команды и прежний manifest XML-сборки не изменились.

[Проверенные входы](prepared-inputs.md) добавляют private proof готового
поколения и отдельную сборочную точку входа. Проверяется полный скопированный
native snapshot до открытия ИБ; schema 2 manifest записывает authority context.
Прежний XML build сохраняет schema 1. [Карта origins](source-map.md) различает
выбранные рабочие/принятые payloads и составное происхождение metadata XML.
[Координаты](source-locations.md) вычисляются по запросу из точных input bytes,
общего CST и native property parser. [Сборочный callback](platform-locations.md)
связывает поддержанные адреса платформы с origins проверенного snapshot.
На установленной 1С проверены предупреждения и ошибка XDTO без строки/колонки;
исходный вывод остаётся доступным, неуспешный импорт не публикует CF/manifest.

Проверенная конфигурация и ленивый project adapter уже подключены к общей
discovery/selection. Expected baseline digest приходит отдельно от настроек;
обычный build не принимает ESKA без protected proof даже при подстановке XML path.
[Byte-preserving transport](git-transport.md) и
[самостоятельная project точка входа](project-cli.md) уже используют это ядро.
[Самостоятельный сборочный CLI](build-cli.md) также использует общее ядро.
Проверка границ зависимостей
повторяется для каждого готового потребителя, не только для общей библиотеки.

Проверки прошли в обеих сборках: fmt/check/strict Clippy, 332 unit и 529
integration для приложения, 138 unit для минимального ядра, Rustdoc с
`-D warnings` и diff check. Обычный прогон пропускает 14 opt-in тестов;
минимальный — три. Предупреждений в принятых сборках нет.
Сборочная библиотека отдельно прошла strict Clippy, 154 unit test и Rustdoc
с `-D warnings`; три opt-in теста пропущены. Все изменения проверены через
обычный полный набор приложения и минимального source core.
Независимый consumer без приложения собрал native C1 на установленной 1С;
SHA-256 CF совпал с manifest. Подробности и ограничения — в отчёте сборщика.
