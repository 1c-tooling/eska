# Подключение существующих ESKA-исходников

`eska-project init` подключает существующий Queries source к новому `eska.toml`
в том же каталоге. Исходники не переносятся, не конвертируются и не переписываются.
Принимаются CF/CFE map 5 и EPF/ERF map 6. Для создания из Designer XML используется
import, а для старого экспериментального layout — явная migration.

```sh
eska-project init existing-project --name demo --format json
eska-project init existing-project --name demo --source authoring \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" --format json
eska-project --project-dir existing-project init --name demo
```

## Источники и целостность

Plan до записи проверяет complete source, фактический тип, UUID и sole version.
Версия CF/CFE читается из YAML, EPF/ERF — из прежнего object-module contract.
Общий read-only `native_version::project_source_version` обслуживает new/init;
старые inspect/bump/release contracts не расширены. Схема версии записывается
без изменения её значения. Platform override необязателен; его отсутствие
сохраняет возможность обычного workspace inheritance.

Существующий accepted store требует independently supplied pin, включая обычный
environment fallback и приоритет флага. Он не пересоздаётся из working edits.
Если store отсутствует, explicit init создаёт candidate base; сам факт создания
не даёт CI approval. При наличии проверенной raw history она сохраняется точно,
включая published state v1 и локальные рабочие правки поверх истории. Если
истории нет, новый проект получает отдельный initial candidate state v2.
Подтверждение support и ancestry остаётся независимым.

Manifest публикуется последним. Existing manifest/aliases, incomplete source,
неверная версия/pin, активный import, повреждённые или несовместимые stores
отказывают. Expected-byte plan привязывает source, existing accepted/history
и transport. Существующий `.gitignore` сохраняется точно; scoped attributes
добавляются после прежней `.gitattributes` policy без потери её исходных строк.
Core не выполняет workflow, Git или workspace enrollment. Producers пишут
accepted/history в отдельный private stage. Generated inventory фиксируется
до exclusive claim публичного store; уже существующий идентичный каталог
не становится собственностью операции. Откат удаляет только точные generated
bytes, включая receipt и незавершённый payload; чужие entries и изменённые
bytes сохраняются с `source_cleanup_failed`.

`ProjectInitializationPlan::verify_published` проверяет первоначальные manifest
bytes/hash и source snapshot перед parent onboarding. Existing accepted/history
сохраняют inventory; новые candidates должны соответствовать starting source.
Intervening edits дают отказ и сохраняются. Настройки после parent workflow
проверяются отдельным exact-byte fence.

## Machine interface

Direct tool envelope остаётся schema 1, `command = "init"`. Data содержит
`root`, `name`, `type`, `metadata_name`, `version`, `version_scheme`, `source`,
`source_format`, `source_layout`, `source_layout_version`, `accepted_base`,
`accepted_base_sha256`, `ancestor_sha256`, `manifest_sha256`,
`accepted_base_created`, `ancestry_preserved`. Пин в ответе — candidate reference,
а не одобрение. Относительный root разрешается от `--project-dir`.

Capabilities добавляет `init`, `initialization_project_types`,
`initialization_source_layout = "queries"`, `initialization_converts_source = false`.
Existing tool command contracts сохраняются. 14 core tests и 4 process группы
прошли, включая all4/RU/EN, candidate/evidence ownership, source-relative path
и exact rollback. Все 22 Rust gates прошли; main onboarding документируется
в связанной следующей части EF-20.
