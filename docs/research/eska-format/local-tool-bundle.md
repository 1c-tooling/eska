# Локальный комплект project/build/check/analyzer и Explorer

`tools/local-bundle.py` собирает согласованный локальный набор отдельных
исполняемых файлов в новый каталог. По умолчанию это `eska`, `eska-project`,
`eska-build`, `eska-check`, `eska-analyzer`. Утилита использует существующие
Cargo recipes, не устанавливает канал обновления и не меняет PATH, профиль
редактора или проекты. Необязательный готовый VSIX Explorer можно добавить при
сборке или упаковать с уже проверенными CLI без Cargo.

Для сборки нужны Rust/Cargo с закреплёнными зависимостями и Python 3.11+
со стандартным [`tomllib`](https://docs.python.org/3/library/tomllib.html).
Готовым бинарникам Python/Cargo не нужны. Проверен Linux x86_64 с Python
3.14.8 и Rust/Cargo 1.98.1. Windows имеет no-replace publication branch,
но runtime-приёмка там не выполнена; остальные ОС отказывают до сборки.

## Создание и проверка

Из корня репозитория, с существующим соседним playground:

```sh
RUN_DIR="$(mktemp -d ../eska-playground/local-bundle-XXXXXX)"
python3 tools/local-bundle.py build "$RUN_DIR/bundle with spaces"
python3 tools/local-bundle.py verify "$RUN_DIR/bundle with spaces"
"$RUN_DIR/bundle with spaces/bin/eska" --version
```

`--profile release|debug` выбирает профиль, release — default. `--repository`
задаёт конкретную копию исходников, `--cargo` — исполняемый Cargo. Все builds
используют `--locked` и отдельный private target; shell/default и минимальные
project/build сохраняют прежние feature boundaries. Checker/analyzer собираются
из самостоятельного SDK workspace с его настоящей экспериментальной версией.

До публикации проверяются package/core versions, команды и форматы,
protocol versions и checker report schema. Shell version проверяется отдельно.
Capabilities probe не открывает проект и не доказывает полноту семантики.
Набор хранит `.eska-local-bundle.json`: состав, платформу, версии, production
source fingerprint, hashes/размеры бинарников и проверенные capabilities.
Receipt не содержит абсолютных путей к исходникам.

`verify` повторяет hashes и bounded probes без записи. JSON результатов и
ошибок независим от locale; help есть на RU/EN (`--lang`, затем `ESKA_LANG`).
Cargo stderr передаётся без изменения. Для probe действуют 64 KiB stdout/stderr
и default 5 s; для каждой сборки — default 1800 s. Пределы можно задать явно.

## Выборочный состав и обновление

```sh
python3 tools/local-bundle.py build "$RUN_DIR/project only" \
  --components eska-project
python3 tools/local-bundle.py build "$RUN_DIR/project updated" \
  --from "$RUN_DIR/project only"
```

Без `--components` новый набор наследует точный состав `--from`, не добавляя
остальные инструменты. Явный `--components` заменяет выбор. Предыдущий набор
сначала проверяется; его receipt hash записывается в новом. Старый каталог
не заменяется и сохраняет bytes/mode/mtime. Это сборка новой версии набора,
а не переключение глобальной установки.

## Необязательный VSIX Explorer

`package` копирует проверенные бинарники и указанный VSIX в новый каталог,
не пересобирая инструменты и не устанавливая расширение в редактор:

```sh
python3 tools/local-bundle.py package "$RUN_DIR/bundle with spaces" \
  "$RUN_DIR/tools and Explorer" --explorer-vsix "$EXPLORER_VSIX"
python3 tools/local-bundle.py package "$RUN_DIR/bundle with spaces" \
  "$RUN_DIR/analyzer and Explorer" --explorer-vsix "$EXPLORER_VSIX" \
  --components eska-analyzer
python3 tools/local-bundle.py verify "$RUN_DIR/analyzer and Explorer"
```

Здесь `EXPLORER_VSIX` — путь к существующему `eska-explorer.vsix`. Без
`--components` сохраняются все CLI исходного набора. Явный выбор допускает
только поднабор уже имеющихся инструментов и требует `eska-analyzer`.
Предыдущий receipt, бинарники и переданный VSIX проверяются; их bytes не
меняются. Новый receipt связывается с предыдущим через
`previous_receipt_sha256`.

VSIX лежит по фиксированному пути `extensions/eska-explorer.vsix`.
Необязательный объект `explorer` в receipt schema 1 содержит этот путь,
hash/размер, `extension_id`, `version`, `vscode_engine` и
`validation: "manifest-and-hash"`. Старые наборы без этого объекта остаются
допустимыми. `verify` повторяет проверку архива вместе с CLI probes.

При сборке `build --explorer-vsix PATH` добавляет явно выбранный VSIX.
`build --from PREVIOUS` по умолчанию наследует прежний VSIX вместе с составом
CLI. Удаление задаётся явно через `--without-explorer`; этот флаг несовместим
с `--explorer-vsix`. Если выбранный состав исключает analyzer, а VSIX остаётся
явным или унаследованным, команда отказывает до Cargo.

Архив читается из одного проверенного regular-file descriptor с пределом
64 MiB фактических bytes; повторное хеширование и копирование также ограничены.
Допускаются не более 10000 ZIP entries, не более 512 MiB суммарного заявленного
распакованного размера и только методы ZIP STORED/DEFLATED. Для
`extension/package.json` действует отдельный предел 1 MiB заявленных и
фактически прочитанных bytes. Повторные имена, выход за каталог, symlink
entries, повреждённые ZIP/DEFLATE и неподходящий manifest отказывают с JSON
ошибкой до публикации. Payload не извлекается и не запускается.

Manifest должен объявлять `1c-tooling.eska-explorer`, версию, VS Code engine,
`./out/extension.js` и настройки analyzer backend. Такая проверка с hash
подтверждает состав и неизменность выбранных bytes. Она не доказывает запуск
расширения, совместимость API с analyzer или полноту анализа; для этого нужна
отдельная установленная клиентская приёмка.

## Публикация и отказ

Source fingerprint связывает Cargo manifests/locks и текущие production inputs
локальных пакетов, включая их paths/bytes/modes. Проверка до/после сборки
блокирует изменившийся источник. Файлы исходников не восстанавливаются поверх
чужих правок. Tests/fixtures и Cargo outputs не входят в production fingerprint.

Полный проверенный набор публикуется одним no-replace rename. Существующий
каталог, файл или broken symlink не заменяются, включая появившиеся во время
build. Ошибка probe, сборки, fingerprint или прерывание оставляют destination
неопубликованным. На Linux дочерняя process group завершается при отмене;
старый комплект остаётся доступным. Чужие байты в destination не удаляются.

## Проверки и границы EF-21

В первой приёмке 18 Python-групп прошли на собственных уникальных playground
fixtures:
full/selective recipes, RU/EN и JSON, compatibility, source changes, bounded
outputs/timeouts, raced destinations, interruption/cleanup и verify без записи.
Это process fixtures с управляемым Cargo, отдельно от настоящей сборки.

Настоящая release-сборка пяти инструментов из frozen source copy прошла
за 937.13 s. Fingerprint исходного дерева до/после копирования совпал со
снимком; изменения продолжающейся разработки не входили в build. Все реальные
probes и `verify` с пустым PATH прошли. Отдельный project-only build и его
обновление через `--from` сохранили выборочный состав и старый набор.

В том же empty-PATH окружении direct project tool создал EPF, standalone checker
и оболочка сохранили побайтно одинаковые stdout/stderr/exit для чистого BSL и
parse findings. EPF syntax честно вернул incomplete/exit 2 из-за отсутствия
полного metadata context. Analyzer прошёл framed initialize, saved diagnostics
и нормальные shutdown/exit. Read-only smoke сохранил проект и весь комплект.

| Бинарник | Версия package | Протокол | Размер, bytes |
| --- | --- | --- | ---: |
| eska | 0.14.2 | version probe | 27195600 |
| eska-project | 0.14.2 | 1 | 8256888 |
| eska-build | 0.14.2 | 1 | 8242800 |
| eska-check | 0.0.0 | 1 | 49073608 |
| eska-analyzer | 0.0.0 | 0.1 | 54476432 |

Linked source core — `0.14.2`; SDK package version не подменяется версией
оболочки. Суммарный размер бинарников — 147245328 bytes.

Normal dependencies измерены в той же frozen copy через locked/offline
`cargo tree --edges normal --target x86_64-unknown-linux-gnu`. Уникальные полные
PackageIds связаны с metadata без неоднозначности; dev/build edges исключены.
Metadata resolution union сохранён отдельно: он включает optional packages,
которых нет в выбранном normal graph. Это граф Cargo, не linker/OS inventory.

| Recipe | Транзитивные packages без root |
| --- | ---: |
| Shell/default | 297 |
| Minimal project + build | 82 |
| SDK check-only | 217 |
| SDK analyzer-only | 217 |
| SDK combined, фактический all5 build | 217 |

В SDK включены библиотеки языкового ядра `ide`/`ide-db`/HIR/Salsa. В его normal
графах и minimal project/build нет обнаруженных GUI/editor runtimes, Tokio,
HTTP/TLS или LSP framework crates; framed LSP реализован собственным transport.
Полный `gix` остаётся в прежнем default shell. `gix-validate` используется
для проверки имён также в минимальных инструментах. Native build требует
отдельно установленную платформу 1С; source/check smoke её не запускал.
[Обезличенный отчёт](local-tool-bundle-results.json) содержит раздельные
runtime cases, графы и проверки сохранности.

## Приёмка упаковки CLI и Explorer

После добавления необязательного VSIX прошли 26 Python-групп: исходные 18 и
8 новых для упаковки, выбора поднабора, наследования/явного удаления VSIX,
проверки архива и manifest, изменения входа, hash/path guards и RU/EN.
Пределы проверены на фактическом чтении, повторном hash и копировании;
повреждённый DEFLATE проверен через настоящий CLI на обоих языках.
Независимый повтор всех 26 групп прошёл за 5.607 s. Это управляемые process
fixtures, включая имитацию Cargo, отдельно от реальной пары ниже.

Существующий release-комплект из пяти инструментов упакован с точным VSIX,
использованным в [установленной приёмке Explorer](explorer-query-syntax.md).
Проверены полный состав и поднабор из одного analyzer. На Linux x86_64
`package` занял соответственно 0.942 s и 0.757 s. Обе команды прошли с пустым
PATH; повторные `verify` на RU/EN дали побайтно одинаковый JSON между локалями.
Cargo не запускался. CLI и VSIX в новых наборах совпали с входами;
bytes/mode/mtime всего исходного набора и bytes входного VSIX сохранены.
Упаковка не меняла профиль IDE.

Обе пары содержат Explorer `0.0.11`, engine `^1.109.0`, размер VSIX
1482456 bytes. Полный набор содержит 147245328 bytes бинарников,
analyzer-only — 54476432 bytes. Это прежние release-бинарники с SDK
`0.0.0` и linked source core `0.14.2`, без новой сборки или повышения версии.
[Отдельный обезличенный receipt](local-tool-bundle-explorer-results.json)
фиксирует эти проверки; исходный receipt 937.13 s / 18 групп сохранён.
Runtime-приёмка Explorer относится к указанной точной паре, а не ко всем
архивам, прошедшим `manifest-and-hash`.

Этот локальный development комплект — часть EF-21. Обычные установщики,
глобальный update/remove и остальные платформы ещё не приняты. Эта проверка
не принимает полную EF-21, EF-18 или MVP: полнота анализа и остальные условия
EF-16 проверяются отдельно.
