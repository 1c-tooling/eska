# Локальный комплект project/build/check/analyzer

`tools/local-bundle.py` собирает согласованный локальный набор отдельных
исполняемых файлов в новый каталог. По умолчанию это `eska`, `eska-project`,
`eska-build`, `eska-check`, `eska-analyzer`. Утилита использует существующие
Cargo recipes, не устанавливает канал обновления и не меняет PATH, профиль
редактора или проекты.

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

18 Python-групп прошли на собственных уникальных playground fixtures:
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

Этот локальный development комплект — часть EF-21. Обычные установщики,
глобальный update/remove, совместная поставка VSIX и остальные платформы ещё
не приняты. Полнота анализа, CF/CFE composition и EF-16 остаются отдельными
условиями MVP; успешная упаковка не меняет их статусы.
