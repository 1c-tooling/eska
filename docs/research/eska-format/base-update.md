# Явное обновление принятой базы

EF-15 выполняется отдельно от повторного импорта EF-25. Библиотечный план
`prepare_base_update` сравнивает три входа: прежнюю принятую базу, независимо
закреплённую новую базу и текущее рабочее дерево. Обычная правка исходника или
импорт пользовательского артефакта не меняют pin и права поддержки.

## Выбор исходников

Сравнение использует SHA-256, размер и объявленные пути. Если рабочий файл не
изменён, выбирается новая поставка. Если поставщик не изменил файл, сохраняется
локальная правка. Одинаковый результат с обеих сторон принимается без конфликта.
Одновременные разные правки одного файла требуют ручного согласования; строки
YAML, BSL и запросов автоматически не сливаются.

Метаданные сопоставляются по UUID, модули и ресурсы — по UUID владельца и native
слоту. Переименование поставщиком переносит локальный модуль только при прежней
identity. Повторное использование имени объектом с другим UUID не переносит
правки. Удаление изменённого файла, локальное удаление обязательного файла,
необъявленные локальные файлы и каталоги дают явные конфликты.

Новая `source.json` берётся из принятой поставки. Рабочая правка её деклараций
отклоняется. Native правила поддержки и вложенные CF остаются самостоятельной
authority; локальная правка этих payload не превращается в новую базу.
Снятие прежнего дополнительного freeze блокирует план. Изменения эффективных
прав показываются отдельно от изменений файлов и pin.

## Защита и публикация

Выбранные значения проверяются тем же механизмом EF-06/EF-08, что и защищённый
экспорт. Новый запрет, несовместимая композиция или неразрешённая ссылка не могут
стать готовым результатом. Любой конфликт запрещает публикацию обоих выходов.

План публикует исходники и новую принятую базу в отдельные каталоги. Прежнее
рабочее дерево и обе входные базы остаются целиком доступны. Новый accepted
store содержит только независимо принятые байты поставки, без локальных правок;
его pin не вычисляется из рабочего дерева. Точное повторение публикации — no-op.
Большие двоичные payload копируются потоково с проверкой отпечатков.

Все три полных входа перепроверяются перед записью и после публикации исходников.
Чужой или изменённый выход, пересечение с входами и filesystem aliases отклоняются.
Библиотечные публикации source/accepted сами по себе не являются общей транзакцией
проектного manifest. Проектная операция ниже связывает их отдельным receipt.

## Проект и CLI

```sh
eska-project --project-dir current-project update-base incoming-project updated-project \
  --accepted-base-sha256 "$APPROVED_OLD_BASE_SHA256" \
  --incoming-base-sha256 "$APPROVED_NEW_BASE_SHA256" --format json
eska-project --project-dir current-project update-base incoming-project updated-project \
  --accepted-base-sha256 "$APPROVED_OLD_BASE_SHA256" \
  --incoming-base-sha256 "$APPROVED_NEW_BASE_SHA256" --write --format json
```

Новая поставка задаётся ESKA-проектом с отдельно принятой базой. Её рабочие
YAML/BSL не используются: операция читает только проверенный accepted store.
Старый pin допускает прежний fallback `ESKA_ACCEPTED_BASE_SHA256`; новый pin
обязателен в аргументе. Проверяются native тип, UUID корня, source profile,
декларации manifest и их неизменность. CI approval не переписывается.

`prepare_project_base_update` публикует отдельный standalone-проект. Выбор
исходного workspace member без whole-workspace режима отклоняется; входную поставку можно выбрать
через `--incoming-project NAME`. В результат копируются только исходники, новая
база и точные `eska.toml`, `.gitattributes`, `.gitignore` исходного проекта.
Git, дополнительные файлы и настройки входной поставки не копируются.
Копирование transport-файлов не проверяет effective Git policy.

Обновление всего workspace включается отдельно:

```sh
eska-project --project-dir current-workspace update-base incoming-workspace updated-workspace \
  --workspace --accepted-bases approved-old.json --incoming-bases approved-new.json \
  --format json
eska-project --project-dir current-workspace update-base incoming-workspace updated-workspace \
  --workspace --accepted-bases approved-old.json --incoming-bases approved-new.json \
  --write --format json
```

Оба файла используют прежнюю schema 1:
`{"schema_version":1,"members":{"project-name":"64 hex characters"}}`.
Каждый список должен точно покрывать свой workspace; один environment pin
не одобряет все проекты. Наборы имён старого и входного workspace должны
совпадать. Входные пути и порядок могут отличаться: сопоставление идёт по имени,
затем отдельно проверяются native identity/profile и pin каждого проекта.
Перенос одного member в standalone по-прежнему не выполняется.

`prepare_workspace_base_update` сохраняет исходные root/member manifests,
transport, пути и порядок проектов. Наследование и явные overrides не заменяются
настройками входной поставки. Конфликт любого member запрещает публикацию всех
проектов. Каждый выход проверяется до первой публикации member;
`.eska-workspace-base-update.json` блокирует общих читателей всех вложенных
проектов до окончания операции, включая уже законченные members. Прерывание
допускает точное возобновление; изменённый member или лишние данные отказывают
до продолжения. Root manifest публикуется последним.

Preview не пишет результат и возвращает exit 0, включая план с конфликтами.
`--write` требует отсутствия конфликтов. Все входы и оба выхода проверяются
до первой записи; изменённый или чужой выход не перезаписывается.
`.eska-project-base-update.json` связывает revisions, два pin и отпечатки точных
settings. Пока marker существует, общие потребители проекта возвращают
`source_update_incomplete`, даже если `eska.toml` уже появился. Точное повторение
возобновляет собственную частичную публикацию; после проверки source/base/settings
marker удаляется. Незавершённый source-файл сначала записывается в private staging,
затем публикуется исключительно. Это восстановление после прерывания операции,
а не обещание общей power-loss транзакции.

После записи для `prepare`/`build` нужен новый independently approved pin.
Прежний проект остаётся доступен со старым pin. Сборка не переключается на новый
проект автоматически; выбор результата остаётся явным.

## Machine interface

Сохраняется tool envelope schema 1: `tool = "eska-project"`,
`command = "update-base"`. Поля `data`:

- `previous_baseline`, `baseline`, `working_revision`, `output_revision`;
- `root`, `incoming_root`, `destination`, `comparison = "whole_file_three_way"`;
- `changes`, `support_changes`, `conflicts`, `can_write`;
- `written`, `publication = null | "created" | "resumed" | "unchanged"`;
- `copied_project_files`, `ci_pin_updated = false`,
  `incoming_working_source_used = false`, `effective_git_policy_verified = false`.

Изменение содержит `previous`, `incoming`, `working`, `selected` с path/bytes/sha256
либо `null`, а также UUID при известном владельце. Support change содержит
UUID и прежнее/новое эффективные права. Conflict kinds: `both_modified`,
`deleted_modified`, `modified_deleted`, `authority_edited`, `permission_denied`,
`freeze_removed`, `unmapped_file`, `unmapped_directory`.

Запись с конфликтами возвращает exit 1 / `source_update_conflicts`,
`error.diagnostics` содержит полный план. Другие отказы включают
`source_update_scope_unsupported`, `source_update_profile_mismatch`,
`source_input_changed`, `source_update_output_mismatch`,
`source_update_receipt_invalid` и существующие ошибки независимо проверенной базы.
JSON одинаков для RU/EN; human строки и stderr локализуются.

Workspace envelope также имеет schema 1 / `command = "update-base"`.
`data.scope = "workspace"`; общие пути, comparison, can_write, written,
publication и три flags сохранены. `data.members` в порядке исходного workspace
содержит name/path/destination и отдельные revisions, два pin, changes,
support_changes, conflicts, can_write и copied_project_files каждого member.
Root settings перечислены в `copied_workspace_files`. При отказе записи полный
план всех members находится в `error.diagnostics`. Несовпадение имён даёт
`source_update_members_mismatch`; неверный/неполный JSON использует прежние
`source_workspace_pins_invalid` / `source_workspace_pins_mismatch`.

## Проверки и оставшаяся часть

Библиотечные сценарии проверяют независимые правки, BOM/CRLF, перенос модуля по
UUID, удаления, повторное использование пути, новый native запрет, freeze,
рабочую правку authority, stale preview и сохранность обоих входов. Синтетический
CF sentinel проверяет перенос authority, а не валидность CF для платформы.

Проектные тесты дополнительно проверяют оба pin, неизменность manifest,
исключение непринятых incoming edits, new-pin prepare, точный повтор и
восстановление частичной публикации с блокировкой читателей. CLI preview/write
проверены на обоих языках. Workspace tests добавляют CF/CFE, разные layouts,
пути/порядок поставок, наследование/overrides, оба pin каждого member,
неверные имена, stale settings, общий отказ и возобновление с outer guard.

Контролируемая пара настоящих CF собрана на Linux / Distrobox, 1С 8.3.27.2325.
В поставке 1.0.0 → 1.1.0 общий модуль переименован при прежнем UUID; локальные
байты BOM/CRLF перенесены в новое имя. Вариант с новым native запретом, локальное
удаление обязательного модуля и пересекающаяся правка версии корня отказаны до
создания результата. Preview одинаков для RU/EN, повтор записи — `unchanged`.

Обновлённый ESKA-проект и независимо изменённый Designer XML прошли build/dump.
14 из 15 файлов совпали побайтно, включая module, ParentConfigurations.bin и
вложенный настоящий CF. Единственное отличие — `configVersion` в
ConfigDumpInfo.xml; names/IDs/структура совпали после удаления только этого
атрибута. Повторная независимая нативная сборка того же input также изменила
только этот служебный атрибут. Source/base inputs сохранены, старый pin
отказан в результате, новый pin проходит strict prepare. Изменённый BSL-модуль
проверен установленным bsl-analyzer-app 0.3.2: complete analysis, без diagnostics.
Workspace MCP здесь недоступен; platform help fallback не подтверждает API,
которых в этом контрольном модуле нет.

Отдельный workspace из двух проектов на той же настоящей CF-паре прошёл RU/EN
preview, публикацию, точный повтор, strict prepare и native build обоих members.
Один member наследует платформу, другой задаёт её явно; root/member settings
совпали побайтно. Входная поставка имеет другой порядок и пути members.
Локальные модули перенесены при supplier rename, а новый native запрет второго
member отказал всей операции до создания первого проекта результата.
Итоговая приёмка MVP, большие поставки и production обновление ИБ остаются
отдельными проверками.

Это управляемые поставки с изменёнными правилами и настоящими native CF,
а не опубликованная производственная пара релизов поставщика. При первичной
распаковке нескольких CF 1С вернула `Invalid data storage format` и обрезанный
дескриптор; операция отказала без принятия/публикации partial XML. Повторная
распаковка тех же байтов прошла. Native сообщение теперь сохраняется при
ошибке проверки descriptor; причина поведения платформы пока не установлена.
Работа в клиенте, обновление production ИБ, большие поставки и другие ОС этой
проверкой не подтверждаются.
