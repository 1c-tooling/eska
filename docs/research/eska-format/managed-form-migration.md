# Миграция в единый form.yaml — EF-12, часть 8

Четыре explicit core APIs используют прежние expected-byte plans и publication:

| API | Вход и результат |
|---|---|
| `prepare_form_layout` | Рабочий source → новая source копия с map 4 |
| `prepare_accepted_form_layout` | Independently pinned accepted blobs → новая база и новый pin |
| `prepare_form_project` | Standalone проект → отдельно мигрированные source/base и точные settings |
| `prepare_form_workspace` | Все declared members → новые независимые pins, прежнее наследование settings |

Прежние targets остаются доступны. Уже существующий map 4 является byte no-op
для прежних layout targets; он не понижается до прежней схемы и не теряет body.
Новый [query target EF-23](query-migration.md) явно переводит map 4 в map 5,
сохраняя definitions, layout и properties.
Открытие проекта не запускает миграцию. CLI выбора нового target этой частью
не добавлен; публичный импорт forms относится к EF-20.

## Преобразование

Layout 1/2 сначала разделяет descriptors, layout 1/2/3 получает прежнюю
relocation/readable проекцию. Native descriptor context определяет declared
managed dialogs независимо от текущего physical payload path. Их рабочие XML
bytes проходят проверенный lossless codec, body добавляется к owner `form.yaml`,
а прежний editable Form.xml исключается из новой source копии.

Metadata UUID/Name/kind, original native fingerprints, модули, templates,
ordinary binary forms, orphan XML и другие payload bytes сохраняются.
Native file map отмечает shared descriptor/dialog owner ролью `managed_form`.
Working descriptor edits и working dialog edits остаются working values.
Та же проекция accepted storage читает только проверенные blobs; текущий source
не становится принятой базой. Supplier CF evidence, native rules и freeze
сохраняются; новый pin возвращается отдельно для явного выбора caller/CI.

При block owner исходный YAML остаётся точным prefix, body добавляется отдельным
ключом. При flow owner новый JSON-compatible value вставляется у closing token
из CST. Перед plan проверяется whole-model equality, включая metadata/native
values, и сохранение comments. LF/CRLF owner сохраняется; mixed owner transport
отказывает. Mixed native XML transport сохраняется как content bytes и не
интерпретируется как YAML. Непроверенный dialog profile отказывает до публикации.

## Публикация

Source и base сохраняют свои independent input/output revisions. Target paths
и bytes видны в preview. Публикуются новые owned directories: повторное
применение не переписывает файлы, прерывание восстанавливается только при точном
receipt и subset ожидаемых bytes. Изменение input или foreign/edited output
вызывает отказ. Старые source/base/settings не изменяются.

Standalone и workspace используют прежние outer receipts; settings/ready
markers публикуются последними. В workspace проверяются pins всех declared
members до начала записи. Root/member manifests сохраняют exact bytes,
platform/workflow inheritance и overrides. Дополнительные пользовательские
файлы, Git state и artifacts имеют прежнюю границу копирования.

## Граница части

Новый editor/protected/map 4 теперь имеют source/base/project/workspace migration.
Позднейшая [итоговая приёмка EF-12](managed-form-acceptance.md) подтверждает
изменённую форму через независимый XML контроль, CF build и свежий platform dump.
CFE/EPF/ERF и готовность всего MVP не заявлены.

## Проверки

Восемь новых тестов покрывают четыре прежних layouts и independent current/base
values, policy/freeze, map 4 no-op без downgrade, exact receipts/repeat/
resume и input/output conflicts, unverified profile, block/flow owner trivia,
LF/CRLF и mixed native transport, standalone settings и whole workspace pins.
Прежний отказ migration map 4 заменён проверкой сохранения authoritative body.

[Результаты C1](managed-form-migration-results.json): перенесены все 823 формы,
source/base имеют по 1 647 изменений (owner, удалённый XML payload и marker).
Восстановленные accepted bodies совпали с независимо прочитанными working bodies
до правок; все Resource permissions сохранены: 186 разрешённых и 637 запрещённых.
Повторная publication и map 4 no-op прошли. Native no-op совпал с независимым
readable контролем по 8 769 файлам и 8 064 каталогам. Две saved правки одной
разрешённой формы — localized Title и root layout — прошли strict protected export;
изменился только её native dialog XML.

Приложение: обязательные fmt/check/strict Clippy/test и Rustdoc прошли, 661 unit,
570 integration, 24 прежних opt-in ignored. SDK all features:
check/Clippy/test/Rustdoc, 359 тестов; CLI project/build: 44/9 тестов.
Результаты независимого реального CF/build/dump цикла — в итоговой приёмке выше.
