# Создание полного проекта с отдельными запросами

Первая часть EF-20 подключает существующий query/form codec к полному creation
pipeline. `eska-project import --layout queries` принимает Designer XML каталог
configuration, extension, external processing или external report и публикует
новый проект с одним редактируемым source.

```sh
eska-project import designer-xml new-project --name demo --layout queries \
  --platform-version 8.3.27.2325 --format json
eska-project --project-dir new-project prepare \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" --format json
```

Распаковка CF/CFE/EPF/ERF остаётся отдельной командой `eska-build unpack`.
Здесь вход — полный XML-каталог; общий `new --from` этой частью ещё не подключён.

## Источники и публикация

`creation::import_query_project` использует `import_designer_queries`, затем
прежние accepted-base, transport и manifest-last шаги. Configuration/CFE получают
source manifest 5, EPF/ERF — 6; descriptor schema 2 сохраняется. Managed form
metadata/body хранятся в одном `form.yaml`, принятые query values — в plain
`.sdbl`. Native paths/UUIDs, metadata name, версия, support и opaque payloads
сохраняются. Имя проекта не переименовывает native root.

`src`, `.eska/accepted`, `.gitattributes`, `.gitignore` и `eska.toml` публикуются
в исключительно новый каталог. Ошибка полного input, query/form profile,
freeze target или публикации удаляет только каталог, захваченный этой операцией.
Занятый destination и родительский input не изменяются. Git и платформа не
запускаются. Отсутствующий platform override остаётся наследуемым для будущего
workspace member.

Вернувшийся SHA-256 — candidate support pin. CI/сборщик получает его независимо;
TOML не сохраняет approval digest. `--freeze` использует прежнюю Resource policy:
запрет достигает managed form и её plain query. Saved edits и protected prepare
используют ту же базу. Initial source ancestor этой частью не сохраняется;
его подключение относится к общему new/init/reimport workflow.

Существующие `import_project`, `import_separated_project`,
`import_readable_project` сохраняют свои layouts и ограничения. CLI default —
`family`; EPF/ERF требуют явный `queries`. Открытие проекта не мигрирует его.
Новых dependencies, settings keys и версии crate нет.

## Machine interface

Envelope schema 1 и семь существующих import data keys сохранены.
`source_layout` дополнен значением `queries`. Capabilities сообщает:

```json
{
  "creation_project_types": ["configuration", "extension", "processing", "report"],
  "import_layouts": ["family", "readable", "queries"],
  "import_layout_project_types": {
    "family": ["configuration", "extension"],
    "readable": ["configuration", "extension"],
    "queries": ["configuration", "extension", "processing", "report"]
  },
  "source_layout_versions": [1, 2, 3, 4, 5, 6]
}
```

Эта map даёт клиенту ограничения выбранного layout. Старые version fields и
значения default import сохранены. Human help/result/error доступны на RU/EN;
JSON keys, layout/type values и candidate pin не зависят от locale.

## Проверки

Три новые core matrix cases и четыре process cases проверяют все четыре root
типа, native alias/name distinction, один form.yaml и два `.sdbl`, exact
BOM/CRLF/opaque bytes и пустые каталоги. Проверены independently opened base,
saved form/query edits под тем же pin, freeze/exclusions, unsupported/malformed
inputs, cleanup и сохранность занятого output. Process tests проверяют RU/EN,
human/JSON, no-TTY, inspect/prepare и точный набор import data keys. Старые
Family/Readable/defaults проходят прежние regression tests.

Все 22 Rust gates прошли: root fmt/check/strict Clippy, default/all-features
tests и Rustdoc, minimal/build-core/build-CLI, SDK и adapter. Root — 817 unit/
600 integration, all-features также 27 build/56 project process cases;
SDK — 187/161/19, adapter — 70 + 12. 24 opt-in root cases пропущены.
Тесты Git-discovery/local fixture HTTP server выполнены с необходимым доступом
после отказов ограниченного sandbox; код проверок не обходил эти отказы.

Linux native matrix использует четыре полных native input. Для каждого отдельно
собраны и выгружены Designer XML control и новый ESKA проект на 1С 8.3.27.2325.
Состав, XML values/UUID/order, BSL text и opaque bytes сравниваются между двумя
выгрузками. Только служебные CDF `configVersion` исключаются; file aliases EPF/ERF
нормализуются по root filename. BSL сравнивается с учётом native BOM/EOL и
завершающих переводов строк; сохранённый ESKA source дополнительно проверяется
побайтно. Native input/settings/pin не меняются.
[Обезличенные результаты](query-project-creation-results.json).

Создание минимального собираемого проекта, общий `new/init`, migration choice,
initial ancestor и делегирование оболочки остаются следующими частями EF-20.
Платформенные saved-edit/runtime/client сценарии этой частью не принимаются.
EF-20 и MVP ещё не завершены.
