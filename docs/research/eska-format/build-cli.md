# Самостоятельный eska-build, EF-10

Экспериментальный `eska-build` использует общий `build_core` и проектную модель.
Оболочка `eska`, Git, IDE и анализатор кода не нужны. Библиотека отвечает за
проверенный snapshot, runner, управляемую временную ИБ и публикацию артефакта
вместе с manifest. CLI выбирает member, читает аргументы и представляет результат.

```sh
cargo build --no-default-features --features build-cli --bin eska-build
```

Default package по-прежнему собирает прежнее приложение. Комплектная поставка
и независимая установка остаются EF-21. Новых dependencies нет: `build-cli`
добавляет существующие runner и CLI features, а также `directories` для тех же
машинных настроек. Source-only и build-only SDK не получают CLI или машинную
конфигурацию. `eska-project` также обходится без runner/config dependencies.

## Команды

```sh
eska-build --project-dir my-project build \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" --format json
ESKA_ACCEPTED_BASE_SHA256="$APPROVED_BASE_SHA256" \
  eska-build --project-dir my-project build --distrobox my-1c-container
eska-build --project-dir my-workspace build --project my-member \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256"
eska-build --project-dir designer-project build --ibcmd /path/to/ibcmd
eska-build capabilities --format json
```

`build` выбирает ровно один проект через ту же discovery/selection, что и
[project CLI](project-cli.md). Workspace root требует `--project NAME`; запуск
внутри member выбирает текущий member. Platform inheritance и каталог артефактов
берутся из общего plan. Результат member относится к workspace output scope.
YAML/метаданные других member не читаются.

Для ESKA отдельно одобренный pin приходит из флага или `ESKA_ACCEPTED_BASE_SHA256`;
флаг имеет приоритет. Сборщик самостоятельно выполняет protected prepare из
сохранённых исходников, а затем проверяет фактический скопированный snapshot.
JSON предыдущей команды `eska-project prepare` не служит proof. При отсутствии
pin, несовпадении базы, неверном YAML или запрещённой правке runner даже не
обнаруживается; ИБ и каталог сборки не создаются. Старый XML не подменяет source.
По умолчанию отказ строгий. `--allow-exclusions` явно разрешает восстановление
по принятой базе, возвращает exclusions и не переписывает рабочие YAML/BSL.
`--generations` задаёт disposable cache, как в project CLI.

Designer XML сохраняет обычную сборку CF/CFE/EPF/ERF через общую библиотеку.
Для него параметры базы ESKA, поколений и exclusions отклоняются, а не
игнорируются. Профиль ESKA поддерживает конфигурацию и расширение Designer 2.20.
Для расширения применяются те же pin, protected prepare и проверка snapshot;
фактический корень должен соответствовать типу артефакта.

`--output` использует общую проверку пути и расширения. `--platform-version`
действует только на этот запуск и не изменяет TOML. `--base-configuration`
доступен для расширения, внешнего отчёта/обработки; относительный CF относится
к выбранному project root. `--recreate-infobase` действует только на принадлежащую
сборщику управляемую ИБ, с проверкой ownership и lock.
Загрузка CF для сборки не подтверждает полноту семантического анализа расширения;
этот контекст подключается отдельно в EF-18.

Машинные runner settings перенесены в общую infrastructure библиотеку.
Действующее приложение и новый tool используют один schema/path resolver:
`ESKA_CONFIG_DIR/config.toml` либо стандартный user config directory ESKA.
Сохранены host/Distrobox/auto, `container`, `platform_arch`, приоритет явных
флагов и переменных `ESKA_IBCMD`, `ESKA_PLATFORM_ARCH`, `ESKA_DISTROBOX`.
Сборщик не пишет эти настройки и требует точную версию платформы.

## Результаты и сообщения

Общие `--lang ru|en`, `--format human|json`, `--project-dir`, help/version
работают без оболочки. Schema 1 совпадает по форме с project tool envelope,
`tool = "eska-build"`, `command = "build" | "capabilities"`. Успех — exit 0,
ошибка аргументов/исходников/runner/платформы/публикации — exit 1.

Успешный JSON содержит artifact/manifest paths, тип артефакта, platform version,
runner, source format, exclusions и platform events. Human stdout содержит
результат; исходные platform bytes поступают в stderr по мере выполнения.
В JSON режиме stdout содержит ровно один envelope. Локализованные ошибки
идут в stderr; native output остаётся в структурированном массиве.

Каждый event содержит `stage`, `stream`, точный массив байтов `bytes` и optional
`origin` из [проверенной карты snapshot](source-map.md). Unknown address или
encoding оставляет `origin = null`; строки/колонки не придумываются.
При ошибке платформы события сохраняются в `error.diagnostics.platform`,
структурная причина — в `error.diagnostics.detail`. Native message bytes сами
по себе могут зависеть от языка платформы; machine keys/codes не локализуются.

Артефакт и manifest публикуются парой после успешного native import. Отказ
сохраняет предыдущую пару. Manifest Designer XML остаётся schema 1, ESKA —
schema 2 с protected context и source map. Git не инспектируется:
`source.git.status = "unavailable"`; отсутствие repository не выдумывается.
Проверка BSL и применение к рабочей БД не являются скрытыми этапами build.

Добавлена [самостоятельная распаковка CF/CFE/EPF/ERF](native-unpack.md) через
`eska-build unpack`. Она не требует проекта и создаёт новый Designer XML каталог.
Designer CheckConfig пока отсутствует; capabilities явно сообщает эту границу. Модель версии нативного manifest
сохраняет текущий contract; расширение x.y.z относится к T83/EF-22.

## Проверки

Девять integration scenarios запускают настоящий отдельный binary и переносимый
native process fixture. Проверены обе локали, stdout/stderr/exit code, byte-preserved
BSL в реальном snapshot, manifest/context, missing/wrong pin до ibcmd, bad YAML,
frozen refusal и явное восстановление, диагностика с origin при успехе/ошибке,
сохранение предыдущей пары, workspace inheritance/output scope, Designer XML,
машинный config, version mismatch и CI pin с одноразовым platform override.
Fixture общий с тестами приложения; synthetic acceptance не заменяет 1С.

Полные fmt/check/strict Clippy/test/Rustdoc и diff check прошли. Приложение:
341 unit + 529 integration; minimal source/build SDK: 147/163 unit;
minimal project/build CLI: 150/169 unit и по девять integration tests.
14 opt-in tests приложения и по три в минимальных ядрах пропущены.
Normal graph build CLI содержит 73 packages без gix engine, HTTP, Tokio,
TTY, updater, архиваторов и IDE/BSL runtime; runner и machine settings доступны.
Release-size сравнение относится к EF-21.

На собственной полной копии C1 отдельный minimal binary собрал CF через
Distrobox / 1С 8.3.27.2325. Артефакт содержит 200 420 573 bytes, SHA-256 совпал
с опубликованным manifest. Snapshot id, protected context, source map и версия
1.0.2.01 совпали с предыдущей платформенной приёмкой. Все пять предупреждений
получили origins из той же карты; source/base/TOML побайтово не изменились.
Проверка не включает применение к БД, клиента или полный Designer CheckConfig.
