# eska development tracker

Этот каталог — рабочая декомпозиция продуктовой спецификации
[`eska-roadmap.md`](../eska-roadmap.md). Исходный документ отвечает на вопрос
«что строим», а этот трекер — «что делать следующим и что уже готово».

## Текущее состояние

По следующему запросу от 2026-10-26 составлен план
**[T134–T142 — компактный конструктор](24-compact-query-constructor.md)**:
табличное редактирование, выражения, связи, матрица UNION, дерево пакета,
модификаторы и связанный текст. T115–T119 уточнены без дублирования;
[изменения bsl-analyzer](../query-analysis-upstream.md) выделены для согласования
с мейнтейнером. Реализация и новый `NEXT` этим планом не назначены.

По запросу от 2026-10-26 созданы задачи **T115–T133**: семантический анализ
запросов и панель полей, гибкое форматирование, редакторы и помощники разработки.
Это backlog для выбора приоритетов; реализация и новый `NEXT` не назначены.
T120–T123 детализируют существующую T26. Развёртывание, отладка и тестовый
контур остаются в T31, T98–T99 и T23 без дублирующих задач.
[Новые направления и зависимости](#задачи-для-выбора-приоритетов).

По решению пользователя от 2026-10-20 развитие продолжается на Designer XML:
самостоятельные `eska-project`, `eska-check`, `eska-build`, `eska-vcs` и общая
делегирующая команда `eska`; `eska-debug` — отдельная опциональная очередь.
`eska.toml` — первичный конфиг, bsl-analyzer — внешний upstream-бинарник без
форка и встраивания его Rust-ядра. Explorer устанавливает project-компонент
и анализатор, использует Metadata API и свой LSP-клиент. При конфликте
приостанавливается свой клиент; два индикатора и обновления независимы.
[План T83–T99](15-designer-xml-analyzer.md) подготовлен от `main`.
По запросу пользователя от 2026-10-20 завершена очередь до T89 включительно.
T90 — `DONE`: сохранён baseline, выделены общее ядро и границы зависимостей.
T83 — `DONE`: схема и read-only resolver контекстов приложений.
T91 — `DONE`: самостоятельный проектный CLI и общие filesystem/presentation операции.
T84 — `DONE`: производный конфиг, миграция с сохранением оригинала и API 1.12.
T92 — `DONE`: project serve, API 1.12 и прямое подключение Explorer.
T93 — `DONE`: самостоятельный eska-check, полнота JSONL, пороги и отчёт CI.
T94 — `DONE`: самостоятельный build/unpack, общий runtime и нативная матрица 8.3.27.2325.
T95 — `DONE`: самостоятельный VCS, общие обработчики и проверенная минимальная установка.
T96 — `DONE`: делегирование общей команды, process-композиция и отделённые runtime-зависимости.
T97 — `DONE`: выборочные ZIP, receipts, совместимость и rollback; приняты минимальные Linux-установки.
T85 — `DONE`: установка компонентов, отдельные индикаторы и обновления Explorer; Linux/VSIX проверены.
T86 — `DONE`: грамматики BSL/SDBL и собственный BSL-клиент; Node/Bun и установленный VSIX проверены на Linux.
T87 — `DONE`: конфликт до и во время старта, остановка своего LSP, реестр install/uninstall и disabled/re-enabled проверены на Linux/VSIX.
T88 — `DONE`: защита правок, XML/BSL refresh, все sources приложения,
protected bridge и два одновременных окна проверены на Linux/VSIX.
T89 — `DONE` на Linux/VSCodium: release-матрица, нативные проверки и полный
CLI прошли. По решению пользователя от 2026-10-22 превышения времени дерева,
старта языка и definition вынесены в T106; исходная серия T89 с failed gates
сохранена. Новая серия T106 прошла все 12 исходных допусков.
[Отчёт и остаток](15-designer-xml-analyzer.md#отчёт-2026-10-22-что-сделано-и-что-осталось).
Номера T83–T89 сохраняются, модульность добавлена в T90–T99.
Экспериментальный формат ESKA в этот план не переносится; статусы прежних
задач сохраняются.

По запросу от 2026-10-21 добавлен [редактор запросов T100–T105](16-query-editing.md):
СКД, динамические списки и BSL в отдельных вкладках, блокировка BSL-владельца,
контекстный анализ черновика и конструктор с сохранением комментариев.
T100 — `DONE`: извлечение/планы XML/BSL, снимки и Metadata API 1.13.
T101 — `DONE`: XML-вкладки, save/recovery, VSIX ru/en и загрузка платформой.
T102–T105 — `DONE` на Linux/VSCodium в ru/en, независимо от опциональной отладки.
Designer XML/BSL остаются исходниками; текущая очередь не меняется.

По решению от 2026-10-22 добавлена [оптимизация T106](17-language-performance.md),
`DONE`: установленные причины исправлены, серия после проверки под lease
прошла 12 из 12 исходных gates на Linux/VSCodium. Предыдущие failed-серии
сохранены в отчёте; остальные среды не проверялись.

По следующему запросу пользователя от 2026-10-22 разрешена очередь
**T106 → T100 → T101 → T102 → T103 → T104 → T105**.
Цель до T105 включительно достигнута: T106 и T100–T105 завершены.
[Приёмка редактора, конструктора, платформы и производительности](16-query-editing.md#t105--приёмка-редактора-и-конструктора).
T98–T99 в эту цель не входят. `DONE` фиксируется только по фактическим проверкам.

По запросу от 2026-10-24 завершена [T107](18-query-constructor.md): пять
вкладок конструктора по образцу Конфигуратора, каталог таблиц и полей текущего
приложения и структурированные операции в ESKA. «Консоль запросов 9000»
используется как референс жизненного цикла конструктора. Core/RPC, Node/Bun,
установленный VSIX ru/en и копия buhmfo приняты на Linux/VSCodium.

По следующему запросу от 2026-10-24 завершена [T108](19-query-advanced.md):
виртуальные таблицы с параметрами, вложенные запросы, объединения и итоги.
Основная логика и модель принадлежат ESKA; адаптер IDE предоставляет интерфейс.
Приняты API 1.23, установленный VSIX ru/en, buhmfo и совместимость с API 1.22
на Linux/VSCodium; другие среды не объявляются проверенными.

По запросу от 2026-10-24 разрешена очередь
**T109 → T110 → T111 → T112 → T113 → T114**:
связанные метаданные базы/расширений, временные таблицы и пакеты, HAVING,
секции итогов, конструкции СКД и приёмка реальной платформой.
T109–T114 завершены; [scope и критерии](20-query-constructor-completion.md).
Вся доменная логика принадлежит ESKA; расширение предоставляет интерфейс.
Редактор T114 принят на Linux/VSCodium ru/en, включая копию buhmfo.
На платформе 8.3.27.2325 прошли 19 сценариев: 16 на минимальной ИБ с данными
и три на изолированной PostgreSQL-ИБ buhmfo со всеми пятью расширениями.
Исправлены DROP и предел одной секции итогов; общий контракт — API 1.30.
[Фактические проверки и ограничения](../measurements/t114-platform-acceptance.json).

Ревью от 2026-10-24 перенесло обратную навигацию по исходникам из Explorer
в общее ядро: Metadata API 1.24 возвращает владельца и цепочку предков одним
запросом. Проверены workspace, Node, установленный VSIX и сравнительные
замеры; [отчёт и ограничения](../reviews/2026-10-24.md).
Это проверка текущей реализации, без открытия следующего milestone.

По запросу от 2026-09-28 в `feat/cli` завершена T82: создание и обновление
проектов из CF/CFE/EPF/ERF. Контракт и состояние — в
[project creation](02-project-creation.md#t82--создание-и-обновление-из-файлов-1с).

Стадия проекта: **локальный CLI MVP и workspace готовы**. По решению пользователя
от 2026-09-17 текущий фокус ветки `feat/ide` — подготовка `eska` и первой версии
VS Code extension для навигации по Designer XML.

- `DONE`: чистый минимальный Rust CLI;
- `DONE`: локализация `ru-RU` / `en-US`, включая `--help`;
- `DONE`: `T01` — минимальная доменная модель проекта;
- `DONE`: `T02` — схема и загрузка `eska.toml`;
- `DONE`: `T03` — project discovery, проверка исходников и ошибки CLI;
- `DONE`: `T05` — встроенные шаблоны для четырёх типов проектов;
- `DONE`: `T04` — `eska new`, клавиатурный TUI, безопасное создание и минимальный Git init;
- `DONE`: `T06` — `eska init`, подключение существующих исходников;
- `DONE`: `T07` — repository layer: HEAD, refs, история, status и changed paths;
- `DONE`: `T08` — workflow policy model, preset overrides и декларативный план;
- `DONE`: `T09` — Trunk preset;
- `DONE`: `T10` — Git Flow preset;
- `DONE`: `T11` — GitHub Flow preset;
- `DONE`: `T12` — `eska status`;
- `DONE`: `T13` — `eska start`;
- `DONE`: `T14` — object-aware human-представление `eska diff`;
- `DONE`: `T15` — `eska save`;
- `DONE`: `T16` — `eska clone` через `gix`;
- `DONE`: `T17` — реализованные VCS-операции переведены на gix-first слой;
- `DONE`: `T18` — локальная история commit/task без fetch и изменения repository;
- `DONE`: `T19` — Designer XML logical object model;
- `DONE`: `T20` — reusable semantic `ChangeSet`;
- `DONE`: `T21` — semantic diff;
- `DONE`: `T22` — генератор commit message;
- `DONE`: `T34` — безопасное переключение между существующими задачами;
- `DONE`: `T40` — локальное завершение задачи;
- `DONE`: `T28` — сборка `.cf`, `.cfe`, `.epf`, `.erf` через
  настраиваемый `ibcmd`, глобальный machine config и одноразовый выбор
  установленной платформы;
- `DONE`: `T42` — спецификация и feasibility-прототип patch-extension `.cfe`;
- `DONE`: `T43` — production-команда `eska patch` для консервативного набора
  изменений методов общих модулей;
- `DONE`: `T25` — чтение и точечное изменение версии проекта 1С;
- `DONE`: `T44` — workspace config, model и discovery;
- `DONE`: `T45` — версионирование workspace members;
- `DONE`: `T46` — сборка workspace members;
- `DONE`: `T47` — VCS UX workspace members;
- `DONE`: `T48` — onboarding workspace members;
- `DONE`: `T29` — `eska doctor`;
- `DONE`: `T49` — стабильные JSON-ошибки и обратимые пути;
- `DONE`: `T50` — анализ только затронутых объектов и явный fallback;
- `DONE`: `T51` — предварительный просмотр `save`;
- `DONE`: `T52` — предварительный просмотр `build`;
- `DONE`: `T54` — паспорт собранного артефакта;
- `DONE`: `T58` — настройка основной production-ветки Git Flow без изменения
  базы feature-веток;
- `DONE`: `T59` — читаемый semantic-вывод `eska diff` с координатами методов,
  устранением дублей и порядком метаданных Конфигуратора;
- `DONE`: `T35` — автоматическое сохранение и восстановление незакоммиченного
  состояния при переключении задач;
- `PLANNED`: `T56` — автоматическая версия релиза 1С и подготовка Release PR/MR;
  сохранена в backlog после переключения приоритета на IDE;
- `IN-PROGRESS`: `T57` — обновление установленного бинарника командой `eska update`
  по отдельному запросу пользователя, без ожидания T56;
- `PLANNED`: `T55` — переносимый стенд реализован, завершение ожидает запусков
  на Windows/macOS и приёмки с настоящей платформой 1С;
- `NEEDS-SPEC`: `T53` — завершение задачи после squash/rebase;
- `PLANNED`: `T37` — `sync` / `continue` / `abort` после T57;
- `PLANNED`: `T26` — `eska fmt`, последняя в очереди последующих CLI-задач;
- `DONE`: `T60` — модель метаданных и логическая identity;
- `DONE`: `T61` — Designer resolver и открытие через обязательный manifest;
- `DONE`: `T62` — fixtures, матрица типов и большой стенд;
- `DONE`: `T63` — выбор XML parsing по измерениям;
- `DONE`: `T64` — разбор Designer XML в модель метаданных;
- `DONE`: `T65` — схемы дерева Конфигуратора;
- `DONE`: `T66` — read-only metadata workspace API;
- `DONE`: `T67` — lazy cache и инкрементальное обновление;
- `DONE`: `T75` — индекс и API поиска;
- `DONE`: `T68` — дисковый кеш и замеры производительности;
- `DONE`: `T69` — спецификация постоянного IDE protocol;
- `DONE`: `T70` — постоянный IDE процесс;
- `DONE`: `T76` — основа расширения и подключение к backend;
- `DONE`: `T77` — дерево, модули и открытие исходников;
- `DONE`: `T78` — поиск и переход к результату в расширении;
- `DONE`: `T79` — фильтр пустых разделов корня и внутри «Общие», исходная настройка;
- `DONE`: `T80` — иконки метаданных;
- `IN-PROGRESS`: `T81` — итоговая приёмка и VSIX;
- `PLANNED`: `T41`, декомпозированная в `T76–T81`, — расширение VS Code с деревом,
  поиском, фильтром, иконками и устанавливаемым VSIX;
- `DONE`: `T71` — архитектура точечного редактирования;
- `IN-PROGRESS`: `T72–T73` — первый набор свойств доступен через общий CLI JSON
  и IDE API 1.8; остаются дополнительные mappings и межплатформенная приёмка;
  `T74` — переименование `IN-PROGRESS`, add/remove `DEFERRED`;
- `T23` test backend и `T39` locking отложены до проверки этого MVP в реальной
  работе.

Практический MVP замыкает основной пользовательский цикл без обязательных
test backend, locking и публикации через `eska`:

```text
start -> status/diff -> save -> switch/return -> finish
                                      |
                                    build .cf
```

| Пользовательская потребность | Текущее покрытие |
|---|---|
| Создать и начать задачу | `eska start <task>` — `DONE` |
| Увидеть изменённые файлы и объекты | `eska status`, `eska diff` — `DONE` |
| Создать commit | `eska save` — `DONE` |
| Переключиться и позднее вернуться | `eska switch` — `DONE` |
| Завершить задачу | `eska finish` — `DONE` |
| Собрать полный нативный артефакт | `eska build` → `.cf/.cfe/.epf/.erf` — `DONE` |
| Собрать patch-extension из delta | `eska patch` → `.cfe` — `DONE` для ограниченного набора методов |

T42 подтвердил узкий сценарий замены метода общего модуля на 8.3.27.2325. T43
зафиксировал консервативный allowlist и добавил отдельную команду с обязательной
проверкой BSL и применимости пакетным Конфигуратором. Подробности и ограничения —
[в результате T42/T43](t42-patch-extension.md).

T35 добавила автоматические полки при `switch` и явные команды
`shelve` / `unshelve` / `shelves`. T58 завершила настройку production-ветки Git Flow с
сохранением `develop` базой обычных feature-веток. T59 сделала semantic-вывод
однозначным, подавила противоречивые события и добавила координаты методов.
Поиск T78, фильтр T79 и иконки T80 реализованы. В работе T81: итоговая приёмка и VSIX. T56 остаётся в backlog.

Структурный рефакторинг после T06: команды сгруппированы в `src/cli/commands/`,
операции проекта выделены в пакеты-владельцы `crates/eska-*`, TOML-схема отделена от проверенных настроек,
TUI разделён на обработку клавиш, отрисовку и управление терминалом. CLI и config
сохранены; T07 не реализуется в рамках рефакторинга. Актуальные пути и границы:
[карта исходного кода](../architecture.md).

Перед началом любой задачи нужно сверять статус с фактическим кодом и менять его
на `DONE` только после выполнения критериев готовности.

Ревью и рефакторинг по запросу 2026-09-24 — **DONE**: исправлено структурное
сравнение XML, ускорено пакетное удаление поисковых ветвей, `diff` и `semantic`
разделены по обязанностям с сохранением CLI/JSON и публичных Rust-путей.
Результаты, охват и ограничения — [в отчёте ревью](../reviews/2026-09-24.md).
Очередность продуктовых задач этим рефакторингом не изменяется.

## Задачи для выбора приоритетов

Запись задач от 2026-10-26 не утверждает предложенную в обсуждении первую очередь.
Прежние статусы и порядок сохраняются до отдельного выбора пользователя.
Языковые возможности развиваются в upstream bsl-analyzer, доменная логика
документов — в ESKA, представление — в Explorer; внешнее ядро не форкается.

| Направление | Задачи | Зависимости внутри направления | Подробности |
| --- | --- | --- | --- |
| Типы, диагностики и панель запросов (Q1–Q5) | T115–T119 | T115 → T116; T118 и T119 требуют T116/T117 | [21-query-analysis.md](21-query-analysis.md) |
| Гибкий fmt (F1–F4), декомпозиция T26 | T120–T123 | T120 → T121/T122 → T123 | [22-configurable-formatting.md](22-configurable-formatting.md) |
| Права на компоненты, редакторы и помощники (L1, E1–E8), XDTO | T124–T133 | T128 требует T125; T129 требует T116/T119; T130–T132 требуют T124 и уточнения scope | [23-metadata-editors.md](23-metadata-editors.md) |
| Компактный конструктор запросов | T134–T142 | T134 → T135; T136 требует T116/T117; T142 объединяет UI и семантическую приёмку | [24-compact-query-constructor.md](24-compact-query-constructor.md) |

T130–T133 имеют статус `NEEDS-SPEC`; остальные новые задачи — `PLANNED`.
Варианты приоритета можно выбирать независимо между направлениями с учётом
зависимостей. T23, T31 и T98–T99 перечислены в
[дальнем списке редакторов и помощников](23-metadata-editors.md).

T134–T142 — `PLANNED`. Рекомендовано начать интерфейс с T134/T135 на текущем
API, независимо согласовать изменения анализатора из T115–T117, затем подключить
семантические функции. Это предложенная последовательность, без изменения `NEXT`.

## Статусы

- `DONE` — реализовано и проверено в текущем репозитории;
- `NEXT` — следующая задача, которую можно брать без дополнительных решений;
- `PLANNED` — задача определена, но ждёт зависимостей;
- `DEFERRED` — намеренно отложенная возможность;
- `NEEDS-SPEC` — идея зафиксирована, но перед реализацией нужна отдельная спецификация.

## Ближайший порядок выполнения

Новая последовательность по запросу от 2026-10-20:
**T90 → T83 → T91 → T84 → T92 → T93 → T94 → T95 → T96 → T97 →
T85 → T86 → T87 → T88 → T89**.
После основной приёмки — опциональные **T98 → T99** для отдельного отладчика.
Фазы, зависимости и критерии приведены в
[плане модульных инструментов](15-designer-xml-analyzer.md).
T90, T83, T91, T84, T92–T97 и T85–T89 завершены.
T89 принята на текущем Linux/VSCodium с решением пользователя вынести
превышения производительности в T106. Полный CLI после исправления лимита
прошёл. T106 после оптимизации прошла все 12 фиксированных performance gates;
старые failed-результаты сохранены отдельно.
Остальные ОС, VS Code и Remote отмечаются непроверенными.
T98–T99 не входят в текущую цель. Ниже сохранена очередь и история первой
IDE-поставки.

Разрешённый пользователем этап после T89 и завершённой T106:
**T100 → T101 → T102 → T103 → T104 → T105** —
[редактор и конструктор запросов](16-query-editing.md).
T100–T105 и последующая T107 завершены. Этап не расширяет
прежнюю приёмку T89 и не зависит от T98–T99.

Порядок изменён пользователем 2026-09-17: ближайший результат — устанавливаемое
расширение VS Code. Завершённые этапы сохраняют `DONE`. Каждая задача имеет
собственные критерии; наличие в очереди не запускает её реализацию автоматически.

| Порядок | ID | Результат |
|---|---|---|
| 1 | T60 | Модель метаданных и стабильная identity — `DONE` |
| 2 | T61 | Manifest, Designer resolver и переход к XML/BSL — `DONE` |
| 3 | T62 | Fixtures, матрица типов и большой стенд — `DONE` |
| 4 | T63 | Выбор XML parser по замерам и проект бюджетов — `DONE` |
| 5 | T64 | Разбор Designer XML по запросу — `DONE` |
| 6 | T65 | Схемы дерева, первая группа модулей и пустые коллекции — `DONE` |
| 7 | T66 | Read-only metadata API — `DONE` |
| 8 | T67 | Lazy cache и обновление затронутых объектов — `DONE` |
| 9 | T75 | Поисковый индекс и core search API — `DONE` |
| 10 | T68 | Дисковый кеш и измеренная производительность backend — `DONE` |
| 11 | T69 | Контракт постоянного IDE protocol — `DONE` |
| 12 | T70 | Процесс `eska ide --stdio` — `DONE` |
| 13 | T76 | Основа расширения и подключение к `eska` — `DONE` |
| 14 | T77 | Дерево, модули и открытие исходников — `DONE` |
| 15 | T78 | Поиск и переход к результату — `DONE` |
| 16 | T79 | Фильтр пустых групп и исходная настройка — `DONE` |
| 17 | T80 | Иконки метаданных — `DONE` |
| 18 | T81 | Приёмка скорости/UI и VSIX; закрытие T41 — `IN-PROGRESS` |

Для всех четырёх типов проекта обязателен `eska.toml`: его проверенный тип
определяет схему дерева. `.bin` без BSL скрываются. Большой стенд —
`<repo>/../eska-playground/big_configuration_temp`; **сборку этого проекта
не запускать**, использовать только чтение метаданных и замеры. Изменения XML
для тестов выполняются на небольших собственных fixtures. Подробные границы и
неподтверждённые UX-предложения — в [плане расширения](14-vscode-extension.md).

T57 выполняется по отдельному запросу пользователя;
T56 → T37 → T55 → T26 остаются очередью последующих CLI-задач;
возврат к ней определяется отдельно после первой IDE-поставки. T55 можно
завершить раньше при доступном runner; это не повод собирать большой IDE-стенд.
T71 завершена; T72–T73 реализованы в ограниченной области и продолжаются.
В T74 пользователь включил переименование со ссылками; add/remove остаются отложенными.

T53 нельзя начинать с ослабления проверки интеграции: сначала снимается
`NEEDS-SPEC`. Уточнение её контракта не блокирует независимые T54–T55.
`publish`, environments, `apply/run` и остальные плановые этапы остаются в backlog;
T23 test backend и T39 locking сохраняют `DEFERRED`.

Детальные контракты улучшений: [T49–T55](12-current-functionality.md).
Контракт автоматической подготовки версии релиза: [T56](10-delivery-and-integrations.md).
Контракт обновления установленного CLI: [T57](10-delivery-and-integrations.md).
Этот план разрешает только отдельно запрошенную задачу, не автоматический запуск
всей очереди.

## Последовательность metadata / IDE

Детальные контракты: [метаданные Designer XML и IDE protocol](13-metadata-and-ide.md).
Это развитие T19/T50 с переиспользованием существующих `ObjectId`, discovery и
mapping, а не повторная реализация завершённых задач. Задачи клиента описаны
в [T76–T81](14-vscode-extension.md), они завершают текущую первую поставку.

| Milestone | Задачи | Результат |
|---|---|---|
| Metadata V0.1 — Core model | T60–T62 | Модель, logical identity, resolver, fixtures |
| Metadata V0.2 — Designer XML parser | T63–T64 | Стратегия parsing и parser четырёх типов проектов |
| Metadata V0.3 — Configurator tree | T65–T66 | Schema, golden tests и read-only API |
| Metadata V0.4 — Incremental workspace | T67, T75, T68 | Lazy cache, поиск, обновления и benchmarks |
| IDE V0.1 — Read-only protocol | T69–T70 | Спецификация и постоянный `eska ide --stdio` |
| VS Code V0.1 — Metadata explorer | T41: T76–T81 | Дерево, модули, поиск, фильтр, иконки и VSIX |
| Metadata V0.5 — Existing object editing | T71–T73 | Safe XML patching и изменение существующих элементов |
| Metadata V0.6 — Structural editing | T74 | Переименование и согласованная запись; add/remove отложены |

Metadata/IDE API первой поставки полностью read-only. T41 — сводная задача
клиента, закрываемая результатом T76–T81 после T70. EDT, MCP, AI, собственный
BSL LSP и визуальный редактор форм в это направление не входят.

## Реестр задач

Таблица сохраняет порядок ID; фактическая очередь приведена выше.

| ID | Статус | Задача | Подробности |
|---|---|---|---|
| B00 | DONE | Clean baseline | [00-foundation.md](00-foundation.md) |
| B01 | DONE | Локализация CLI | [00-foundation.md](00-foundation.md) |
| T01 | DONE | Доменная модель `Project` | [01-project-foundation.md](01-project-foundation.md) |
| T02 | DONE | Схема и загрузка `eska.toml` | [01-project-foundation.md](01-project-foundation.md) |
| T03 | DONE | Project discovery и validation | [01-project-foundation.md](01-project-foundation.md) |
| T04 | DONE | `eska new` | [02-project-creation.md](02-project-creation.md) |
| T05 | DONE | Built-in templates | [02-project-creation.md](02-project-creation.md) |
| T06 | DONE | `eska init` | [02-project-creation.md](02-project-creation.md) |
| T07 | DONE | Repository layer (`gix`; документированный Git capability fallback) | [03-repository-workflow.md](03-repository-workflow.md) |
| T08 | DONE | Workflow policy model | [03-repository-workflow.md](03-repository-workflow.md) |
| T09 | DONE | Trunk preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T10 | DONE | Git Flow preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T11 | DONE | GitHub Flow preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T12 | DONE | `eska status` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T13 | DONE | `eska start` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T14 | DONE | `eska diff` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T15 | DONE | `eska save` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T16 | DONE | `eska clone` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T17 | DONE | Gix-first миграция реализованных VCS-операций | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T18 | DONE | `eska history` | [05-safe-vcs.md](05-safe-vcs.md) |
| T19 | DONE | Designer XML object model | [06-locking-and-xml.md](06-locking-and-xml.md) |
| T20 | DONE | Semantic `ChangeSet` | [07-semantic-changes.md](07-semantic-changes.md) |
| T21 | DONE | Semantic diff | [07-semantic-changes.md](07-semantic-changes.md) |
| T22 | DONE | Генератор commit message | [07-semantic-changes.md](07-semantic-changes.md) |
| T23 | DEFERRED | Спецификация test backend | [08-quality.md](08-quality.md) |
| T24 | PLANNED | `affected` analysis | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T25 | DONE | Versioning проекта 1С | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T26 | PLANNED | `eska fmt`, сводная T120–T123 (прежний порядок сохранён) | [08-quality.md](08-quality.md) |
| T27 | PLANNED | `eska check` | [08-quality.md](08-quality.md) |
| T28 | DONE | Build через `ibcmd` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T29 | DONE | `eska doctor` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T30 | PLANNED | Development environments | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T31 | PLANNED | `apply` / `run` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T32 | PLANNED | Release pipeline | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T33 | PLANNED | CI integration | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T34 | DONE | `eska switch` | [05-safe-vcs.md](05-safe-vcs.md) |
| T35 | DONE | `shelve` / `unshelve` / `shelves` и автоматическое восстановление при `switch` | [05-safe-vcs.md](05-safe-vcs.md) |
| T36 | PLANNED | `eska restore` | [05-safe-vcs.md](05-safe-vcs.md) |
| T37 | PLANNED | `eska sync` / `continue` / `abort` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T38 | PLANNED | `eska publish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T39 | DEFERRED | Locking объектов | [06-locking-and-xml.md](06-locking-and-xml.md) |
| T40 | DONE | `eska finish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T41 | PLANNED | eska: 1C Explorer — сводная задача T76–T81 | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T42 | DONE | Спецификация и прототип patch-extension `.cfe` из разницы веток | [t42-patch-extension.md](t42-patch-extension.md) |
| T43 | DONE | Генерация patch-extension для ограниченного набора методов | [t42-patch-extension.md](t42-patch-extension.md) |
| T44 | DONE | Workspace config, model и discovery | [11-workspaces.md](11-workspaces.md) |
| T45 | DONE | Versioning workspace members | [11-workspaces.md](11-workspaces.md) |
| T46 | DONE | Build workspace members | [11-workspaces.md](11-workspaces.md) |
| T47 | DONE | Workspace-aware `status`, `diff` и `save` | [11-workspaces.md](11-workspaces.md) |
| T48 | DONE | `new` и `init` для workspace members | [11-workspaces.md](11-workspaces.md) |
| T49 | DONE | Стабильные JSON-ошибки и обратимые пути | [12-current-functionality.md](12-current-functionality.md) |
| T50 | DONE | Анализ затронутых объектов и явный fallback | [12-current-functionality.md](12-current-functionality.md) |
| T51 | DONE | Предварительный просмотр `save` | [12-current-functionality.md](12-current-functionality.md) |
| T52 | DONE | Предварительный просмотр `build` | [12-current-functionality.md](12-current-functionality.md) |
| T53 | NEEDS-SPEC | Завершение задачи после squash/rebase | [12-current-functionality.md](12-current-functionality.md) |
| T54 | DONE | Паспорт собранного артефакта | [12-current-functionality.md](12-current-functionality.md) |
| T55 | PLANNED | Переносимые проверки сборки; ожидает внешней приёмки | [12-current-functionality.md](12-current-functionality.md) |
| T56 | PLANNED | Автоматическая подготовка версии релиза 1С | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T57 | IN-PROGRESS | Обновление установленного `eska` | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T58 | DONE | Настраиваемая основная production-ветка Git Flow | [03-repository-workflow.md](03-repository-workflow.md) |
| T59 | DONE | Читаемый semantic-вывод `eska diff` с координатами методов | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T60 | DONE | Модель метаданных и логическая identity | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T61 | DONE | Designer path resolver | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T62 | DONE | Набор Designer XML fixtures | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T63 | DONE | Выбор стратегии XML parsing | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T64 | DONE | Разбор Designer XML в модель метаданных | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T65 | DONE | ConfiguratorSchema и ConfiguratorTree | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T66 | DONE | Read-only metadata workspace API | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T67 | DONE | Lazy index, cache и инкрементальное обновление | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T68 | DONE | Производительность metadata workspace | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T69 | DONE | Спецификация постоянного IDE protocol | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T70 | DONE | Постоянный read-only IDE процесс | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T71 | DONE | Архитектура safe XML patching | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T72 | IN-PROGRESS | Точечный XML patch и безопасная запись | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T73 | IN-PROGRESS | Редактирование существующих элементов | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T74 | IN-PROGRESS | Переименование со ссылками; add/remove отложены | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T75 | DONE | Поиск по метаданным | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T76 | DONE | Основа расширения и подключение к eska | [14-vscode-extension.md](14-vscode-extension.md) |
| T77 | DONE | Дерево проектов, модули и открытие исходников | [14-vscode-extension.md](14-vscode-extension.md) |
| T78 | DONE | Поиск в расширении и переход к результату | [14-vscode-extension.md](14-vscode-extension.md) |
| T79 | DONE | Фильтр пустых групп и настройка исходного режима | [14-vscode-extension.md](14-vscode-extension.md) |
| T80 | DONE | Иконки метаданных | [14-vscode-extension.md](14-vscode-extension.md) |
| T81 | IN-PROGRESS | Производительность интерфейса, приёмка и VSIX | [14-vscode-extension.md](14-vscode-extension.md) |
| T82 | DONE | Создание и обновление из файлов 1С | [02-project-creation.md](02-project-creation.md) |
| T83 | DONE | Контекст приложения в основном конфиге ESKA после выделения ядра | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T84 | DONE | Производный конфиг bsl-analyzer и миграция | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T85 | DONE | Установка project/analyzer из Explorer, независимые индикаторы и обновления | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T86 | DONE | Базовая подсветка и собственный LSP-клиент Explorer | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T87 | DONE | Приостановка клиента при конфликте расширений | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T88 | DONE | Защита правок, XML/BSL refresh, все sources приложения, protected bridge и два окна | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T89 | DONE | Linux/VSCodium: минимальные установки, CLI/LSP и buhmfo; превышения производительности по решению пользователя вынесены в T106 | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T90 | DONE | Cargo workspace, общее ядро проекта и границы зависимостей | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T91 | DONE | Самостоятельный eska-project и общие правила проекта | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T92 | DONE | Сервер метаданных eska-project serve и переход Explorer | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T93 | DONE | Статическая проверка CI через eska-check и внешний bsl-analyzer | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T94 | DONE | Самостоятельный eska-build и проверки реальной платформой | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T95 | DONE | Самостоятельный eska-vcs с сохранением workflow | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T96 | DONE | Делегирование команд eska самостоятельным инструментам | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T97 | DONE | Выборочные пакеты, установка и совместимость инструментов | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T98 | PLANNED | Опциональная проверка DAP backend и контракт отладчика | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T99 | PLANNED | Опциональный самостоятельный eska-debug | [15-designer-xml-analyzer.md](15-designer-xml-analyzer.md) |
| T100 | DONE | Извлечение запросов XML/BSL и обратная запись без потерь | [16-query-editing.md](16-query-editing.md) |
| T101 | DONE | Вкладки XML-запросов, save/recovery, VSIX ru/en и загрузка платформой | [16-query-editing.md](16-query-editing.md) |
| T102 | DONE | BSL-черновики, защищённый снимок и явное применение | [16-query-editing.md](16-query-editing.md) |
| T103 | DONE | Контекстный анализ черновика внешним bsl-analyzer | [16-query-editing.md](16-query-editing.md) |
| T104 | DONE | Конструктор запросов с сохранением комментариев | [16-query-editing.md](16-query-editing.md) |
| T105 | DONE | Приёмка редактора запросов, сохранности и производительности | [16-query-editing.md](16-query-editing.md) |
| T106 | DONE | 12 исходных performance gates пройдены на Linux/VSCodium; прежние failed-серии сохранены | [17-language-performance.md](17-language-performance.md) |
| T107 | DONE | Пять вкладок, таблицы/поля приложения и общий backend; установленный VSIX ru/en и buhmfo приняты на Linux/VSCodium | [18-query-constructor.md](18-query-constructor.md) |
| T108 | DONE | Виртуальные таблицы с параметрами, вложенные запросы, объединения и итоги в ESKA; установленный VSIX ru/en, buhmfo и API 1.22 приняты на Linux/VSCodium | [19-query-advanced.md](19-query-advanced.md) |
| T109 | DONE | Связанные и наследуемые метаданные базы и расширений в каталоге ESKA | [20-query-constructor-completion.md](20-query-constructor-completion.md) |
| T110 | DONE | Временные таблицы, INTO, порядок операторов и зависимости пакета | [20-query-constructor-completion.md](20-query-constructor-completion.md) |
| T111 | DONE | Отбор по агрегатам ИМЕЮЩИЕ/HAVING в ESKA и интерфейсе | [20-query-constructor-completion.md](20-query-constructor-completion.md) |
| T112 | DONE | Одна исполнимая секция итогов с несколькими уровнями | [20-query-constructor-completion.md](20-query-constructor-completion.md) |
| T113 | DONE | Типизированные конструкции языка СКД по грамматике платформы | [20-query-constructor-completion.md](20-query-constructor-completion.md) |
| T114 | DONE | 19 платформенных сценариев, PostgreSQL buhmfo с пятью расширениями и редактор ru/en | [20-query-constructor-completion.md](20-query-constructor-completion.md) |
| T115 | PLANNED | Типы значений характеристик и составные ссылки (Q1) | [21-query-analysis.md](21-query-analysis.md#t115--типы-значений-характеристик-и-составные-ссылки-q1) |
| T116 | PLANNED | API семантической модели запроса (Q2) | [21-query-analysis.md](21-query-analysis.md#t116--api-семантической-модели-запроса-q2) |
| T117 | PLANNED | Контекстные диагностики XML и BSL-запросов (Q3) | [21-query-analysis.md](21-query-analysis.md#t117--контекстные-диагностики-xml-и-bsl-запросов-q3) |
| T118 | PLANNED | Панель источников, полей и результата (Q4) | [21-query-analysis.md](21-query-analysis.md#t118--панель-источников-полей-и-результата-q4) |
| T119 | PLANNED | Параметры запроса и их ожидаемые типы (Q5) | [21-query-analysis.md](21-query-analysis.md#t119--параметры-запроса-и-их-ожидаемые-типы-q5) |
| T120 | PLANNED | Политика и схема настроек форматирования (F1) | [22-configurable-formatting.md](22-configurable-formatting.md#t120--политика-и-схема-настроек-форматирования-f1) |
| T121 | PLANNED | Гибкий форматтер BSL (F2) | [22-configurable-formatting.md](22-configurable-formatting.md#t121--гибкий-форматтер-bsl-f2) |
| T122 | PLANNED | Гибкий форматтер SDBL и встроенных запросов (F3) | [22-configurable-formatting.md](22-configurable-formatting.md#t122--гибкий-форматтер-sdbl-и-встроенных-запросов-f3) |
| T123 | PLANNED | Единый fmt для CLI, CI и Explorer (F4) | [22-configurable-formatting.md](22-configurable-formatting.md#t123--единый-fmt-для-cli-ci-и-explorer-f4) |
| T124 | PLANNED | Происхождение компонентов и права на выбранные редакторы (L1) | [23-metadata-editors.md](23-metadata-editors.md#t124--происхождение-компонентов-и-права-на-выбранные-редакторы-l1) |
| T125 | PLANNED | Создание, копирование и удаление метаданных (E1) | [23-metadata-editors.md](23-metadata-editors.md#t125--создание-копирование-и-удаление-метаданных-e1) |
| T126 | PLANNED | Редактор прав ролей (E2) | [23-metadata-editors.md](23-metadata-editors.md#t126--редактор-прав-ролей-e2) |
| T127 | PLANNED | Состав подсистем, планов обмена и общих настроек (E3) | [23-metadata-editors.md](23-metadata-editors.md#t127--состав-подсистем-планов-обмена-и-общих-настроек-e3) |
| T128 | PLANNED | Заимствование и перехватчики в расширениях (E4) | [23-metadata-editors.md](23-metadata-editors.md#t128--заимствование-и-перехватчики-в-расширениях-e4) |
| T129 | PLANNED | Генерация BSL по запросу (E5) | [23-metadata-editors.md](23-metadata-editors.md#t129--генерация-bsl-по-запросу-e5) |
| T130 | NEEDS-SPEC | Редактор табличных макетов (E6) | [23-metadata-editors.md](23-metadata-editors.md#t130--редактор-табличных-макетов-e6) |
| T131 | NEEDS-SPEC | Визуальный редактор управляемых форм (E7) | [23-metadata-editors.md](23-metadata-editors.md#t131--визуальный-редактор-управляемых-форм-e7) |
| T132 | NEEDS-SPEC | Редактор схемы компоновки данных (E8) | [23-metadata-editors.md](23-metadata-editors.md#t132--редактор-схемы-компоновки-данных-e8) |
| T133 | NEEDS-SPEC | Редактор XDTO | [23-metadata-editors.md](23-metadata-editors.md#t133--редактор-xdto) |
| T134 | PLANNED | Компактная оболочка и адаптивная компоновка | [24-compact-query-constructor.md](24-compact-query-constructor.md#t134--компактная-оболочка-и-адаптивная-компоновка) |
| T135 | PLANNED | Табличное редактирование и групповые действия | [24-compact-query-constructor.md](24-compact-query-constructor.md#t135--табличное-редактирование-и-групповые-действия) |
| T136 | PLANNED | Контекстный редактор выражений | [24-compact-query-constructor.md](24-compact-query-constructor.md#t136--контекстный-редактор-выражений) |
| T137 | PLANNED | Связи и сложные условия | [24-compact-query-constructor.md](24-compact-query-constructor.md#t137--связи-и-сложные-условия) |
| T138 | PLANNED | Матрица объединений | [24-compact-query-constructor.md](24-compact-query-constructor.md#t138--матрица-объединений) |
| T139 | PLANNED | Дерево пакета, временные и виртуальные таблицы | [24-compact-query-constructor.md](24-compact-query-constructor.md#t139--дерево-пакета-временные-и-виртуальные-таблицы) |
| T140 | PLANNED | Настройки и модификаторы запроса | [24-compact-query-constructor.md](24-compact-query-constructor.md#t140--настройки-и-модификаторы-запроса) |
| T141 | PLANNED | Связанный текст, комментарии и просмотр изменений | [24-compact-query-constructor.md](24-compact-query-constructor.md#t141--связанный-текст-комментарии-и-просмотр-изменений) |
| T142 | PLANNED | Приёмка компактного конструктора | [24-compact-query-constructor.md](24-compact-query-constructor.md#t142--приёмка-компактного-конструктора) |

Отложенные и пока недостаточно определённые возможности перечислены в
[99-deferred.md](99-deferred.md). Общие правила для каждой задачи находятся в
[working-agreement.md](working-agreement.md).

## Как обновлять трекер

1. В начале работы пометить одну задачу `NEXT` как `IN-PROGRESS` в её файле и таблице.
2. Не расширять scope за границы задачи; новые идеи записывать в `99-deferred.md`.
3. После реализации выполнить общий Definition of Done.
4. Пометить задачу `DONE`, записать принятые решения и сделать следующую
   разблокированную задачу `NEXT`.
