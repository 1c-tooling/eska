# Текущие роли и ресурсы прав

`eska-analysis`, `eska-analyzer` и `eska-check` используют один canonical input
роли. YAML задаёт исходные UUID и Name; объявленный `Rights.xml` — отдельно
версионированную зависимость. Обе части берутся из общего `SourceReader`.
XML дескриптор роли не генерируется, платформа и Git не запускаются.

## Ввод и модель

Открытие сессии регистрирует pending resources без чтения payloads. Методы
`role_rights_paths`, `role_rights_owner` и `role_rights` дают объявленную связь
и текущие readiness/origin/revision/hash. `load_role_rights_saved`,
`apply_role_rights_overlay` и `close_role_rights_overlay` используют тот же clock,
проверку версии, bounded reader и source-byte резерв, что YAML/BSL.

Текст ресурса хранится verbatim в `Arc<str>`, с BOM/CRLF. Его `Ready` означает
доступность текущих UTF-8 bytes. При проекции роли native `parse_rights_xml`
получает именно этот текст; отдельной сохранённой копии `RoleData` нет.
Правка YAML использует текущий ресурс, правка ресурса перепроецирует его роль.
Соседние canonical slots не изменяются. Kernel `resolve_role` использует этот
slot; declared unavailable/wrong-kind value не читает legacy XML.

Роль без объявленного ресурса получает явно пустой `RoleData`. Pending, missing,
encoding/size/quota failure или invalid XML объявленного файла не подменяются
пустыми правами. Invalid XML остаётся текущим читаемым текстом, но canonical роль
становится unavailable с `analysis_role_rights_invalid` и точным resource path.
Диапазон YAML для этой ошибки не выдумывается. Ошибка самого YAML сохраняет
прежнее поведение input/adapter; права её не исправляют.

`load_semantic_inputs` и metadata preparation пакетной syntax-проверки загружают
pending/transiently cancelled saved resources. Editor inputs и устойчивые ошибки
сохраняются до явного изменения. Invalidation preflight проверяет весь пакет;
сохранённый ресурс становится pending без IO, слот роли очищается, editor input
остаётся авторитетным. Close читает актуальный диск, включая deletion/failure.

`resources.metadata_source_bytes` теперь учитывает owning YAML и raw rights
ровно по одному разу; прежние JSON keys сохраняются. Все входы делят default
256 MiB source quota и per-document bound 64 MiB. Invalid XML занимает свой
байтовый резерв, недоступный текст его освобождает. Native parser/model heap,
temporary allocations, Salsa memos и RSS этим счётчиком не ограничиваются.

## Сервер и checker

В `experimental.eska` добавлен `roleRightsSync=true`, protocol остаётся `0.1`.
Full document open/change/save/close принимает только declared rights resources.
Прежний предел transport frame — 4 MiB с JSON overhead; saved reader может
читать документ до 64 MiB. Эти пределы различаются и для больших `Rights.xml`.
`eska/inputChanged`/`eska/inputsChanged` использует `input.kind=role_rights`,
поле `owner` и обычные origin/hash/input summary; raw XML в событие не копируется.
Dynamic saved watcher включает exact basename права, обычно `**/Rights.xml`;
посторонние совпадения по-прежнему игнорируются только negotiated watcher.

`eska/metadata/read` и metadata pull готовят одну pending saved dependency роли,
без чтения остальных ресурсов. Refresh YAML не перезаписывает открытые права.
Read/owner page показывает canonical failure в `model_issue`:

```json
{"code":"analysis_metadata_dependency_unavailable","failure":{"code":"analysis_role_rights_invalid","location":["Roles/Reader/Ext/Rights.xml"]}}
```

YAML diagnostics остаются диагностикой YAML: ошибка XML не получает фиктивный
YAML finding. Доступность YAML и полнота модели различаются; failed resource
сохраняет `metadata_model_complete=false`. XML pull diagnostics здесь нет.
Checker передаёт ту же причину в metadata issue с resource location и null range;
читаемый YAML сохраняет свой hash и массив собственных findings. Неполнота даёт
exit 2 и не удаляет уже найденные ошибки BSL. Machine output одинаков RU/EN.

## Проверки и пределы

Новые проверки охватывают pending/absent distinction, native resolver, UUID,
flags/refs/RLS, resource-only overlay, соседние slots, retained old snapshot,
stale/unknown/exhausted requests, shared quota, cancellation, current disk close,
saved invalidation и оба языка настоящего stdio/checker. Отдельный kernel test
подтверждает адресную инвалидацию и отсутствие XML fallback.

[Машинный отчёт](analyzer-roles-results.json) содержит gates и реальные результаты.
Все 46 gates прошли: 279 tool tests, native 2627/20 ignored, adapter 64/7,
root 899/14 ignored; strict Clippy/Rustdoc и feature matrix включены.
Все 212/676/753 модели C1/C2/C3 совпали с native reader по identity, трём флагам,
223 334/29 458/155 734 ссылкам и 66/4 055/6 495 условиям RLS. Проверены hashes
850/2 706/3 014 source/native/control files; данные не изменились. Шесть stdio
сценариев проверили current rights, invalid/close, watcher deletion/recreation
на owned partial copies с полным неизменённым composition. Остальные payloads
в этих copies намеренно отсутствовали; полной семантической приёмки это не даёт.

Checker C1 проверил прежние 1 975 модуля и сохранил все 26 SelfAssign findings.
212 отказов Role исчезли, осталось 1 285 metadata issues; остальные issues
не изменились. Rights добавили 70 735 893 байта к общему резерву; текущий total
185 945 711 байт остаётся в default quota. JSON RU/EN одинаков, hashes всех
7 829 файлов сохранены. PATH был пустой, платформа не запускалась.

Native модель хранит ссылки/RLS для анализа зависимостей; она не вычисляет
эффективные права, не интерпретирует permission values и не проверяет RLS runtime.
Native XML parse не имеет внутренних cancellation checkpoints; отмена чтения и
публикации кооперативная. Экспериментальное Rust API получило `CanonicalMetadata::Role`
и отдельные resource inputs; published source schemas и версии не менялись.
EF-18 остаётся `IN_PROGRESS`: остальные модели, общий memory budget, edits и
Explorer ещё в работе.
