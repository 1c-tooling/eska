# Сборка ESKA через оболочку

`eska build` делегирует сборку одного ESKA проекта отдельному `eska-build`.
Materialization, support pin, protected snapshot, native runner и публикация
остаются в существующем build core. Designer XML использует прежний маршрут
и прежние JSON-схемы.

```sh
eska --project-dir demo build \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" --format json
eska --project-dir workspace build -p main \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" \
  --platform-version 8.3.27.2325 --distrobox 1c-ubuntu-env --format json
```

## Процесс и аргументы

Оболочка сначала выбирает проект и проверяет общий read-only build plan.
Aggregate/multiple/mixed selection, `--workspace` и ESKA `--dry-run` явно
отклоняются до запуска companion. `--manifest` допустим: прямой build всегда
публикует artifact/manifest. Для preview protected source используется
`eska-project prepare`; это не preview native execution.

Поиск начинает с sibling рядом с текущим executable, затем PATH. Найденный
несовместимый sibling не заменяется другим экземпляром. Capabilities probe
проверяет tool/envelope schema 1, exact package version, protocol 1, `build`,
`eska` и выбранный artifact type. Probe ограничен пятью секундами и 64 KiB
stdout, закрывает stdin/stderr и завершает/reaps собственный child при ошибке.
Reader с унаследованной descendant pipe не задерживает отказ бесконечно.

В реальном build stdin/stdout/stderr наследуются. Успешный JSON и build failure
JSON имеют прямой `eska-build` envelope; оболочка не переписывает его. Ошибки
собственного selection/preflight/probe используют прежний shell build error
envelope. Numeric child status сохраняется, включая Windows i32; Unix signal
становится `128 + signal`. Parent не держит publication transaction или legacy
interrupt handler; cleanup принадлежит завершённому build child.

Передаются canonical locale, выбранный member, platform/native context,
recreate flag, output и новые `--accepted-base-sha256`, `--generations`,
`--allow-exclusions`. Pin environment/default/precedence проверяет прямой CLI.
`--select-platform` сохраняет прежний selector, передавая выбранную version.
Относительный output сохраняет project/workspace scope; base CF предварительно
разрешается по прежнему shell workspace rule. Generations остаётся
project-relative; OS paths передаются через `OsString` без shell.
Явные ESKA source options на Designer XML дают отказ до платформы.

## Проверки

12 process cases включают actual facade/companion с portable ibcmd на CF/CFE/
EPF/ERF, RU/EN human/JSON, exact saved BOM/CRLF/non-BMP bytes, pin/manifest,
workspace member/inheritance/output/base path. Isolated sibling fixtures
проверяют argv, raw stdout/stderr, exit 23, Unix signal 143, missing tool,
PATH priority, wrong version/protocol/tool/profile, malformed/oversized JSON,
failed probe и timeout. Старые 41 build и 4 format-refusal cases проходят.

Все 22 Rust gates прошли: root 821 unit/610 default integration (612 на
all-features), 27 build/56 project process, SDK 187/161/19, adapter 70 + 12,
minimal/build-core/build-CLI и strict Rustdoc. 24 opt-in root cases пропущены.
Четыре настоящих проекта прошли facade build/dump на 1С 8.3.27.2325. Native
состав 15/109/11/6 файлов равен независимому Designer XML control с одной
ожидаемой local BSL comment правкой. XML values/UUID/order и opaque bytes
совпали; исключён только служебный CDF configVersion. Saved source bytes/pin/
settings и последующий первый reimport проверены отдельно.

Четыре изменённых модуля проверены CLI fallback bsl-analyzer-app 0.3.2:
complete, failed files 0 и SyntaxError 0. Workspace MCP недоступен; в собственных
demos bsl-analyzer.toml отсутствует, использованы defaults. Остальные style/context
findings не подтверждают production semantic coverage.
[Обезличенные результаты](shell-build-delegation-results.json).

Physical terminal Ctrl+C родителя, aggregate ESKA build, Windows execution и
устанавливаемый комплект этой частью не принимаются. Общие new/init/import
и приёмка MVP ещё в работе; crate version и pipeline не менялись.
