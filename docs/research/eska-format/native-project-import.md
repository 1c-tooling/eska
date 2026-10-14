# Native staging и проектный план импорта

Это четвёртая часть EF-25, после
[project journal/recovery](source-import-recovery.md). Новая библиотечная связка
соединяет распаковку CF/CFE/EPF/ERF с существующим проектом без второй модели
слияния или поддержки. Native CLI lifecycle и новая карта ещё не реализованы.

`PreparedArtifact::prepare_source_import(project, scope, approved, ancestor,
cancelled)` возвращает `PreparedArtifactSourceImport`. Handle заимствует
артефакт: его native staging остаётся живым до завершения preview/application.
`plan()` показывает прежний `ProjectSourceImportPlan`, `publish()` проверяет
native proof и делегирует запись этому же журналу. Ошибки разделяют
`ArtifactSourceImportError::Artifact` и `Project` с прежними structured failures.
Новый `verify_cancellable` не принимает частичный native snapshot.

Project core отдельно предоставляет `prepare_project_native_source_import`
для complete Designer XML input и нового принадлежащего операции staging.
Для conversion выбирается существующий approved layout 1–6; открытие не
мигрирует source/base. Проверяются выбранный member, UUID, полный CF/CFE/EPF/ERF
тип и независимо переданный CI pin. Результат проходит прежний protected selector;
правила поставщика не принимаются через импорт.

Для EPF/ERF проверенный XML root filename и весь directory prefix приводятся
к pinned native root. Filename остаётся обратной привязкой, metadata Name/UUID
и bytes BSL/opaque payloads не меняются. Обрабатываются Unicode и dotted stems;
слияние по новой карте из-за metadata rename/add/delete всё ещё блокируется.

Копирование закреплено исходным inventory. Captured incoming восстанавливается
и экспортируется обратно: состав, empty directories, BSL и непрозрачные ресурсы
должны совпасть точно. Только известный regenerated XML сравнивается по ordered
элементам, attrs, namespace bindings и значениям. Допускается оформление XML
и отступ в пустом MD `ChildObjects`; whitespace-only scalar остаётся данными.
Проверка сравнивает восстановленный output с captured package и исходные XML
строки с их digest, поэтому новый mutable staging не назначается proof.

Native inventory добавлен к rechecks исходного source plan, в том числе при
доступе к `plan()` напрямую. Receipt содержит его optional `native_revision`.
Входящие service versions `ConfigDumpInfo.xml` остаются в raw ancestry,
а working source сохраняет прежнее pinned bookkeeping. Это не выдаёт ему
Resource permission и не меняет защиту `ParentConfigurations.bin`/supplier CF.
После применения recovery использует уже сохранённый converted incoming;
исходный native staging не требуется. Сбрасывание preview удаляет только
его новый projection; журнал, старые исходники и accepted storage остаются.

Отмена работает в inventory/copy и между conversion/capture/round-trip этапами.
Существующие conversion, capture и export имеют отдельные непрерываемые этапы;
это не обещание немедленной отмены внутри каждого parse/write. До guarded
publication исходники проекта не меняются. Public CLI этой частью не расширен.

## Проверки

Модельные сценарии проверяют readable CF, EPF/ERF со сменой native filename,
сохранение local module, wrong UUID/type, invalid XML, cancellation, foreign
staging и stale native input. XML comparator различает scalar whitespace,
значения, element order и namespace bindings. Build-core consumer проверяет
заимствование proof, удаление projection и отказ при изменённом native staging.

Все 22 Rust gates прошли: root 786 unit/600 integration, all-features также
21 build/50 project process cases, SDK 185/158/19 и adapter/minimal build
checks. 24 opt-in root случая пропущены и не считаются подтверждёнными здесь.

Native consumer подготовил отдельное изменение metadata Comment для четырёх
контрольных проектов и собрал входящие CF/CFE/EPF/ERF через `eska-build`.
Каждый артефакт прошёл настоящий `PreparedArtifact::unpack`, conversion и
guarded publication с сохранением независимого local BSL. Полученный ESKA
проект собран с исходным CI pin и повторно выгружен на 1С 8.3.27.2325.
13/107/8/4 native файлов и 3/25/3/1 BSL modules совпали: BSL/opaque bytes точно,
XML по структуре/значениям при исключении только service `configVersion`.
Имена входящих EPF/ERF содержали Unicode и точки. Для обработки явно задан
контрольный базовый CF. [Обезличенные результаты](native-project-import-results.json).

Реальный CF выявил два уточнения, покрытые регрессиями: отступ в пустом
`ChildObjects` не является scalar data; fresh `ConfigDumpInfo.xml` не должен
приобретать Resource permission. Исходники при обоих первоначальных отказах
сохранились. Полные успешные cycles выполнены после исправлений.
Workspace MCP недоступен; CLI анализатора 0.3.2 завершил проверку четырёх
изменённых native modules без SyntaxError и failed files. Есть 2/5/50/2
style/context diagnostics; полной семантики или выполнения клиента это
не подтверждает. EF-25 остаётся `IN_PROGRESS`.
