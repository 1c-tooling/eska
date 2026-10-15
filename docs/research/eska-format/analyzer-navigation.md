# Навигация и права чтения общего анализатора

`eska-analyzer` передаёт объявленный состав выбранного ESKA-проекта через тот же
сеанс, который принимает YAML/BSL и выполняет языковые запросы. Новый XML
backend или клиентский parser метаданных для этого не требуется.

## Контракт

Initialize добавляет `experimental.eska.project`: `project_type`, `scope`
(`standalone` либо `member` с именем), `root_uri`, `source_uri`, `root_source`.
URI указывают на выбранный проект и его единственный редактируемый source.
Эти поля сообщают объявленную идентичность, а не готовность semantic models.

`eska/metadata/owners` сохраняет прежние snapshot/state/native-path поля и
добавляет для каждого физического owner:

- `declaration`: opaque `id`, `kind`, `name`, `uuid`, `parent`;
- `modules`: объявленные `source`, `native_path`, `native_descriptor`, `role`,
  `binary`;
- `queries`: объявленные `source`, `native_resource`.

Состав фиксируется из проверенного SourceReader до передачи его в SDK. Получение
деклараций не читает owner payloads, BSL, `.sdbl` или accepted storage. Current
properties и координаты остаются в прежних `read`/`locate` запросах.

`eska/metadata/permissions` принимает текущий `snapshot` и от 1 до 200 уникальных
объявленных `sources`. Ответ содержит тот же snapshot и `files`: `source`,
`read_only`, `unknown`, `objects` с текущими policy results. Capability отдельно
указывает `readOnly`, `cancellable` и независимый accepted-base pin.

Pin приходит только из `--accepted-base-sha256` или
`ESKA_ACCEPTED_BASE_SHA256`. Semantic scope, editable manifest и generated
candidate его не заменяют. Без pin состояние неизвестно и файл read-only.
Pending/invalid owner также не восстанавливает разрешение по старым saved/base
данным. Для YAML физический файл закрыт для записи, если хотя бы один его
inline object запрещает операцию; заморозка ESKA учитывается отдельно.

Policy request проверяет текущий сеанс, source declarations, независимую базу,
отмену и deadline. Import/control changes завершают прежний сеанс и освобождают
navigation/policy state. Запрос не меняет source, не включает edit plans и не
придаёт LocalSyntax semantic authority.

## Граница готовности

Пять настоящих framed groups прошли: DTO/declarations четырёх типов, current
dirty input, independent pin/freeze, invalid params/snapshot, deadline и import
retirement. Navigation helpers — 7, policy helpers — 4; publication/cancel/stream
— 3, прежние syntax/property controls — 6. SDK all-features прошёл полностью:
205 unit, 177 analyzer, 24 checker, 7 navigation и 4 policy tests. Отдельные
неподключённые query helpers также прошли 7 tests и в эту возможность не входят.
Strict all-target/all-feature Clippy, SDK check/fmt, warnings-denied Rustdoc и
diff-check прошли. Native filename role matcher проверен отдельно; import
нестандартного регистра filenames этим не заявляется.

Это backend-контракт для дерева и readonly свойств. Его публикация не означает
приёмку установленного Explorer, query analysis или CF/CFE composition. Полная
приёмка четырёх типов проектов и оставшиеся функции Explorer фиксируются отдельно.
