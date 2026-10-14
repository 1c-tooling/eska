# Queries migration с сохранением истории

`eska-project migrate --layout queries` переносит standalone или весь workspace
в новую копию с managed forms и отдельными `.sdbl`. Default Readable, старые
Rust APIs и Readable JSON сохраняются. Migration работает без платформы и Git.

```sh
eska-project --project-dir old-project migrate copied-project --layout queries \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" --format json
eska-project --project-dir old-project migrate copied-project --layout queries \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" --write --format json
eska-project --project-dir old-workspace migrate copied-workspace --layout queries \
  --workspace --accepted-bases approved-bases.json --write --format json
```

## Рабочие исходники и поддержка

Source и independently pinned accepted base преобразуются раздельно. Рабочие
YAML/BSL, local edits, UUID, ресурсы и порядок состава не становятся новой
поддержкой. Settings и transport копируются точно; source layout определяется
по фактическому результату каждого плана: 5 для CF/CFE, 6 для EPF/ERF.
Вернувшийся pin необходимо отдельно выбрать для следующих prepare/build.

Если raw import history отсутствует, migration сохраняет это отсутствие и не
создаёт ancestor из текущих правок. Readable CF/CFE переходят к Queries; уже
современные внешние проекты сохраняют map 6. Прежние scoped transport rules
и independent base/freeze сохраняются.

## Raw history

При layout no-op полностью копируется `.eska/source-import`: initial state v2,
published state v1, raw stores, journals, backups и receipts. Active snapshot
проверяется против отдельно открытой принятой базы по schema/type/root UUID.
Это проверка целостности, не подтверждение ancestry или recovery.

Весь archive inventory входит в migration fingerprint, receipt, recheck и
resumable publication. State публикуется после его данных. Изменённый original
или partial output отказывает до последующей записи. Старые digests и receipts
не пересчитываются; raw ancestry сохраняет прежний независимый ключ.

Если source или accepted representation изменится при существующей raw history,
возвращается `source_migration_ancestry_unsupported` до создания destination.
Несовместимый старый snapshot не выдаётся за историю нового layout. Для workspace
проверяются все members до публикации первого. Перенос истории между различными
layouts требует отдельного подтверждённого контракта и здесь не реализован.

## Machine interface и проверки

Queries standalone добавляет `source_layout = "queries"`, фактический
`source_layout_version` и `ancestry_preserved`. Workspace показывает отсортированный
набор `source_layout_versions`; каждый member сообщает свою версию и preservation.
Capabilities содержит `migration_layouts = ["readable", "queries"]`.
Envelope schema 1, прежние Readable keys и values остаются прежними.

35 core migration и 12 process cases прошли адресные проверки. Они покрывают
все четыре типа, mixed workspace maps 5/6, обе локали, human/JSON, независимую
базу/freeze/local edits, initial/published history no-op, malformed/foreign/
changed storage, symlinks, fingerprint, interrupted resume и общий workspace
отказ без output. Все 22 Rust gates прошли. [Native результаты](query-project-migration-results.json)
подтверждают четыре history-preserving map 5/6 копии и два перехода Readable
CF/CFE map 3 → 5 без истории на 1С 8.3.27.2325: независимые build/dump совпали,
original source/base/settings и local BSL сохранились. Перенос истории между
layouts остаётся явным отказом; клиентское исполнение этой частью не принимается.
