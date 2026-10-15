# Текущая CF/CFE composition в общей сессии анализа

Срез EF-18 от 2026-10-15 подключает явно настроенные CF/CFE исходники к одному
`AnalysisSession`, `RootDatabase` и native workspace snapshot. CF и расширения
получают отдельные `SourceRootId`; YAML owners, nested forms и BSL используют
один checked allocator `FileId`. Одинаковый относительный путь разных проектов
больше не обозначает один файл. XML materialization, второй индекс, отдельный
host или новая схема `eska.toml` для анализа не добавлены.

Это поддержка объявленного контекста и текущих вкладов. Полная EF-18, контекстная
семантика отдельных `.sdbl` в EF-27 и общая приёмка MVP этим срезом не закрываются.

## Настройки и выбранный проект

Состав задаётся существующим `bsl-analyzer.toml` выбранного проекта. Например,
в проекте расширения B, рядом с его `eska.toml`:

```toml
[source]
root = '../base/src'
extensions = [
  { name = 'A', path = '../a/src' },
  { name = 'B', path = 'src', depends_on = ['A'] },
  { name = 'Sibling', path = '../sibling/src' },
]
```

`root` и `extensions[].path` разрешаются относительно фактического
`Project.root` выбранного проекта, в том числе workspace member. Конфигурация
`--config settings/check.toml` у checker также выбирается относительно этого
корня; её расположение не меняет базу разрешения source paths. Работают текущие
`ProjectConfig.resolve_configuration_path` и `ExtensionTopology`, без отдельного
разбора TOML. Используются structured extension entries с явными name/path и
dependency closure. В этом примере B видит base, A и собственный вклад;
зарегистрированный Sibling не входит в его семантическую область.

CLI принимает source directories проверенных ESKA проектов. Admission
проверяет тип CF/CFE по SourceReader profile и сохраняет actual root UUID/name.
Также проверяются наличие выбранного корня, duplicate/overlapping canonical paths,
повторные header UUID и корректность графа зависимостей. Нет неявного поиска
базы, glob discovery, native XML loader или восстановления отсутствующего пути.
External roots, dependency-store declarations и source exclusions этим
consumer не принимаются. EPF/ERF сохраняют отдельный
[собственный контекст](external-own-context.md).

Обычный CF сохраняет single-root route. CFE без объявленного CF-контекста
не превращается в Configuration: `syntax` использует прежний LocalSyntax,
а непринятый semantic scope отказывает. Public admission API
`composition::admit` и `AnalysisSession::from_composition` доступны с feature
`analyzer-cli` или `check-cli`; базовые `RootId`/`SourceKey` и методы текущих
входов остаются в default library. Разделение features намеренное.

## Текущие вклады и идентичность

Декларации и tracked slots регистрируются до первого запроса. Их наличие
не означает, что YAML/BSL уже прочитаны. Перед semantic query загружаются все
текущие видимые metadata/resource/BSL inputs через существующий bounded
`InputRequest`. Холодные completion, definition, references, signature, hover
и local rename используют тот же request cancellation/deadline. Read-only SDK
query не выполняет скрытое чтение: библиотечный caller сам подготавливает inputs.

Ready snapshots не перечитываются. Editor overlay, включая принятый неудачный
ввод, сохраняет authority. Invalid/pending/unavailable видимый вклад не позволяет
выдать прежнюю базу за готовый результат: запрос получает
`analysis_composition_incomplete` с root-qualified source. Checker оставляет
модули unchecked и возвращает exit 2 при неполном текущем составе.

Native UUID и `ExtendedConfigurationObject` определяют наследование принятого
объекта. Одинаковое имя его не заменяет. Тест меняет только current UUID-ссылку
заимствованного объекта: base field исчезает, close возвращает сохранённую связь
и поле. Dirty/invalid YAML расширения не меняет исходную модель CF и saved bytes.

`SourceKey { root, source }` различает одинаковые authoring paths. Методы `_at`
принимают такой ключ для YAML, ресурсов, BSL и queries; они используют общий
`SessionRevision` и source-byte quota. Замена переиспользует резерв своего
входа. Stale version/revision отказывает без изменения; принятые quota/cancel
failures очищают прежние bytes/hash. Close читает текущий диск. Source quota
по-прежнему не является heap/RSS budget.

Прежние методы с `SourcePath`, metadata navigation и language caller URI
остаются facade выбранного корня. Foreign inputs принимаются по их declared
URI; definition/reference targets несут root и URI фактического владельца.
Это не обещает произвольные foreign language requests через selected-root API.
`RootId` имеет смысл только вместе с конкретной сессией; после reopen требуется
новый session snapshot. Экспериментальный Rust API изменён: module results и
navigation targets теперь содержат root; compatibility layer не создавался.

## References, controls и протокол

`ReferenceArea.candidate_roots` — узкое дополнение native API. SDK передаёт только
видимые `SourceRootId`. Ядро объединяет их штатные name-usage candidates,
сортирует и дедуплицирует FileId до file cap, затем применяет прежний `files`
intersection и подтверждает semantic symbol identity каждого попадания.
`None` сохраняет прежний anchor-root route; пустой explicit набор не даёт
workspace candidates. File-local symbols сохраняют собственный scope.
Ограничения 200 candidate files и 10 000 показанных hits остаются явными:
`files_capped`/`hits_capped` не дают принять сокращённый результат за полный.

Control inventory объединяет source manifest, root descriptor, project settings,
analysis settings, import receipt и фактический workspace каждого admitted
проекта, включая невидимый sibling. Foreign workspace берётся из результата
project selection, не выводится по родителям source directory. Его событие
`workspace/didChangeWatchedFiles` требует restart так же, как selected controls.
Перед публикацией host и при обработке запросов проверяются import receipts
всех admitted проектов; отсутствие первого receipt не скрывает следующий.

Control/import retirement освобождает прежний host и публикует
`eska/restartRequired` с последним опубликованным snapshot. Следующие запросы
получают `project_restart_required`. Drafts принадлежат клиенту; новый процесс
получает их только при явном replay. Saved watcher инвалидирует точный
root-qualified input, не заменяя открытый overlay. Обычные control changes
требуют доставки watcher event; нового фонового наблюдателя здесь нет.

Protocol 0.1 и checker schema 1 сохранены. Initialize добавляет
`experimental.eska.composition`, batch report — optional `composition`:
`source_sha256`, `selected`, `roots`, `current_complete`. Каждый root сообщает
source identity, label, visibility, input/model/module readiness и retained
source bytes. `current_complete` относится к текущим поддержанным metadata/BSL
вкладам, а не к семантике всех ресурсов. Root добавлен к input acknowledgements,
saved invalidations и definition/reference targets. Для настроенной composition
hover/signature capability указывает `inputPreparation=visible-current-composition`.
Machine keys/ranges не зависят от locale. Одиночные проекты сохраняют прежний route.

## Проверки и границы доказательства

Fixture создаёт небольшие native XML CF/CFE, импортирует их production Queries
pipeline и запускает настоящий analyzer/checker subprocess. Это синтетические
проекты и настоящий framed transport, без mock host. Нового запуска 1С,
проверки установленного Explorer или большого CF/CFE corpus здесь не было.

Проверены 8 admission/identity, 4 direct host, 5 framed и 2 checker группы.
Framed проверки включают RU/EN, одинаковые module paths, dirty adoption,
invalid current без base fallback, saved reload, холодные definition/references
после invalidation, foreign metadata targets, local rename, foreign workspace
retirement и поздний import невидимого sibling. Checker сравнивает полный JSON
RU/EN, exit 0/2, missing adopted input и config в подкаталоге. Native references
имеет ещё 6 новых tests и 84 прежних regression tests в узких filters.

SDK fmt, check default/all-features, strict all-target Clippy в обеих комбинациях
и diff-check прошли. Полный default набор: 184 tests; all-features: 464
(231 library, 192 analyzer, 30 checker, 7 navigation, 4 policy). Root/adapter full
matrix не включена в этот receipt. Независимое read-only ревью не нашло blockers
в просмотренных composition routes; оно не запускало повторные tests.
Точные команды, counts, cases и исключения:
[analyzer-cf-cfe-composition-results.json](analyzer-cf-cfe-composition-results.json).

Standalone `.sdbl` остаются [SyntaxOnly](analyzer-query-syntax.md): metadata и
consumer completeness false. Selected BSL profile coverage не означает полную
семантику платформы, DCS/dynamic-list consumer validation или runtime correctness.
Для complete semantic queries необходимы все поддержанные текущие видимые
вклады; непокрытый либо недоступный вклад не получает silent fallback.
