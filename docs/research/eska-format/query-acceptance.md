# Приёмка отдельных запросов — EF-23

EF-23 — `DONE` для configuration/Designer 2.20 и managed logform 2.20 на
1С 8.3.27.2325. [Результаты](query-acceptance-results.json) подтверждают критерии
1–3 и платформенную часть пункта 6 [контракта](sdbl-queries.md).
Контекстный анализ запросов и потребителей, IDE/batch parity и unsaved analysis
остаются отдельной задачей EF-27. CLI defaults пока прежние; map 5 выбирается
явно через библиотечный API.

## Хранение и защита

Native codec и references версии 1 сохраняют полный текст пакета СКД и
произвольного запроса списка. Dataset определяется полной цепочкой names,
список — ID корневого Attribute; пути файлов не заменяют эти identities.
Единственный редактируемый text живёт в `.sdbl`, reference markup полностью
удаляется при материализации. Остальные native resource bytes сохраняются.

| Проверка | Подтверждение |
|---|---|
| Native no-op, quoting, EOL, XML entities и комментарии | [Codec](native-query-codec.md) и [references](query-references.md), все 823 формы/51 СКД C1 |
| Несколько scopes, unions, rename и collisions | Строгие reference fixtures; полный declared set, явные отказы missing/duplicate/inline fallback |
| Project import/export и addressed reader | [Map 5](query-project.md), 153 resources/150 plain files C1 |
| Supplier rights/freeze и native bindings | [Protected export/source map 4](query-protected.md); 46 allowed/104 denied C1, independent control |
| Правка одного значения | [Saved/preview editor](query-edit.md); только адресный `.sdbl`, byte/pin/storage conflicts и Resource permission |
| Явная миграция | [Source/base/project/workspace](query-migration.md); independent pins, exact settings, CST, no-op/repeat/resume |

Миграция C1 сохранила рабочие inline-values отдельно от исходной принятой базы.
Оба полных native вывода проверены по 8 769 files/8 064 directories. Protected
selection сохранил права и дал 41 changed resource, равный независимому контролю.
Исходные source/base/control и settings остались прежними.

## Платформа и выполнение

В собственном малом playground Q1 создана конфигурация с тремя элементами
справочника, отчётом СКД и управляемой формой с сохранённым dynamic list.
Исходное условие `Код >= "02"` вернуло `02`, `03` в обоих случаях.
После opt-in migration editor изменил только два `.sdbl`: условие стало
`Код >= "03"`. Native no-op до правок совпал с прежним export; после правок
изменились ровно два native resources; оба дерева из 11 files/12 directories
сверены целиком. Контрольные query values подготовлены отдельно от YAML/editor.

Контрольный Designer XML и protected ESKA собраны через `eska-build build`,
затем каждый CF выгружен через `eska new --from` в новый каталог. Checksums
проверены по manifests; ESKA build использует approved pin и source map 4,
strict export имеет ноль exclusions. Свежие dumps совпали по 11 files и
12 directories, кроме девяти служебных `ConfigDumpInfo.xml` configVersion.
Другие bytes этого файла равны. Decoded conditions после платформы — `>= "03"`;
source references отсутствуют, версия конфигурации `1.0.0.1` сохранена.

Для исполнения каждый CF загружен и применён к своей owned ИБ через `ibcmd`:
команда apply ESKA ещё не реализована. Настоящий клиент 1С запущен в Xvfb.
Из compiled report получена схема СКД, из открытой таблицы формы — её
исполняемые схема и настройки. Обе выполнены процессором компоновки данных
с выводом в таблицу значений. Контрольный и ESKA маршруты вернули только `03`.
Loose `.sdbl` во время работы конфигурации не читаются. Это исполнение
compiled ресурсов; визуальная вёрстка отчёта и интерактивные действия не приняты.

Missing query отказал до платформы с `source_query_value_missing`; прежние CF
и manifest сохранились. После восстановления проверены exact source/base bytes
и настройки проекта. BSL CLI проанализировал оба модуля без failed files и
errors/warnings; три stylistic hints сохранены. Workspace MCP был недоступен,
справочный MCP и реальные API установленной платформы использованы отдельно.

Последние общие Cargo/Rustdoc и SDK gates прошли для кода миграции:
708 unit/570 integration, 359 SDK tests; 24 прежних ignored. Для этой
документации проверены JSON, ссылки и diff; повторный Cargo-прогон не требуется.
Остальные native profiles, C2/C3 и CFE/EPF/ERF принимаются отдельно.
