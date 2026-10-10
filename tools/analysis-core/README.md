# Общая сессия анализа ESKA

Библиотека `eska-analysis` связывает общий контроллер saved/overlay, адаптер
метаданных и локальное ядро BSL. Она находится в отдельном workspace; минимальная
проектная библиотека не зависит от анализатора. Платформа, Git, IDE-сервер и
XML-экспорт не участвуют в чтении.

Состав берётся из проверенного manifest и configuration snapshot. При открытии
все остальные YAML остаются pending, а их идентичности и slot стабильны.
Сохранённые файлы и черновики используют одни преобразователи. Неверный текущий
ввод очищает канонический slot; прежняя модель не используется как fallback.
Результат kernel resolve и координаты имеют явную ревизию. Общий `SessionRevision`
учитывает каждое принятое изменение YAML и BSL, включая неудачное чтение;
внешний потребитель должен дополнять его идентичностью сессии после reopen.

Пока подключены главные дескрипторы уже реализованных адаптеров. Полнота чтения,
полнота главных моделей и покрытие всех вложенных документов различаются.
`metadata` возвращает текущий YAML-снимок для дерева и свойств, включая виды,
для которых BSL-проекция ещё не поддержана. Объявленные текстовые модули проходят
saved/overlay/close; diagnostics и completions используют те же байты и
канонические метаданные. Бинарные и непрочитанные тела имеют явное состояние.
Лимит текущих загруженных текстов — 64 MiB по умолчанию; это не бюджет RSS.
Общая [квота текущих входов](../../docs/research/eska-format/source-budget.md)
считает исходный размер готовых YAML-моделей и BSL-текстов, по умолчанию 256 MiB.
Замены переиспользуют свой резерв; недостаточная квота очищает прежний вход.
`kernel_memory()` возвращает отдельный снимок таблиц Salsa: число живых entries,
размеры полей и optional heap estimate. Отсутствующий hook отличается от нулевой
оценки; сумма может повторно учитывать общие Arc и не включает всю память процесса.
Снятие статистики не вычисляет запросы и не меняет текущую ревизию.
`QueryCancellation` прерывает выполняющийся запрос на ближайшем checkpoint
Salsa; один токен управляет одним запросом и не меняет исходники или ревизию.
Первый самостоятельный checker выполняет явный профиль `syntax` с кооперативным
deadline. Первый IDE transport читает метаданные этой же сессии. Общий бюджет
памяти, полный сервер и семантическая проверка ещё не реализованы.
Библиотека не опубликована и не входит в установленный комплект.

Read-only пример `project_session` принимает каталог исходников ESKA, читает
только объявленных владельцев и проверяет результат каждого kernel resolver
точным равенством с текущей моделью. JSON раздельно показывает полноту чтения,
главных моделей и вложенных дескрипторов; неполное покрытие даёт exit 2.
Запуск не создаёт XML и не требует установленных Git/платформы/IDE.

Пример `project_module` читает метаданные и один объявленный общий модуль,
сравнивает syntax-only диагностику saved/editor/close с отдельной batch-сессией.
Неполнота метаданных или модулей остаётся явной и даёт exit 2; пример не является
публичной командой проверки проекта. Экспериментальные методы сессии теперь
принимают `SessionRevision` вместо внутреннего clock контроллера метаданных.

```bash
ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test --manifest-path tools/analysis-core/Cargo.toml --locked
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_session -- /path/to/eska/source
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_module -- /path/to/eska/source
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_memory -- /path/to/eska/source
```

Команды выполняются из корня `eska`; для тестов путь задаётся к соседнему
playground. Подробности и реальные замеры —
в [результатах сессии](../../docs/research/eska-format/shared-session.md)
и [проверке BSL](../../docs/research/eska-format/bsl-session.md).
Контракт отмены — в [проверке запросов](../../docs/research/eska-format/query-cancellation.md).
Пример `project_memory` читает все YAML и до 40 объявленных текстовых модулей;
сравнивает оценки до/после освобождения текста и глубокой LRU-очистки.
[Реальный замер](../../docs/research/eska-format/kernel-memory.md) показывает
ограничения этих оценок; очистка не гарантирует освобождение всех memo или RSS.

## Самостоятельная проверка

Feature `check-cli` собирает `eska-check` в этом же отдельном workspace.
Проверка читает выбранный проект ESKA и `bsl-analyzer.toml`, использует те же
BSL diagnostics и метаданные, что сессия IDE. Объявленные модули вложенных команд
привязаны к YAML владельца, без отдельного command XML. Необъявленные ресурсы
не становятся активными модулями при обходе каталога.

```bash
cargo build --manifest-path tools/analysis-core/Cargo.toml --features check-cli --release --locked --bin eska-check
```

В каталоге выбранного проекта запускается собранный бинарник:

```bash
eska-check --profile syntax --format json
eska-check --profile syntax --project main --timeout-seconds 300
eska-check --profile syntax --max-retained-source-bytes 268435456
```

Его пока нет в установленном комплекте `eska`; путь к собранному бинарнику
нужно указать самостоятельно. Exit codes: 0 — проверенный профиль без находок,
1 — ошибки синтаксиса, 2 — неполная проверка, 3 — ошибка запуска или настроек.
Неполнота метаданных не скрывается даже при отсутствии syntax findings.
Профиль `syntax` не проверяет типы и платформенную исполнимость. Текст каждого
модуля освобождается после проверки; это правило не подходит для семантического
профиля. [Контракт и реальные результаты](../../docs/research/eska-format/check-cli.md).

## IDE transport метаданных

Отдельная feature `analyzer-cli` собирает `eska-analyzer`. Feature `check-cli`
не включает сервер; библиотека без обеих features работает без транспорта и CLI.
Бинарник использует общий bounded framing/JSON parser, не прежний handshake
приложения. Runtime не зависит от LSP crates, Git, платформы или отладчика.

```bash
cargo build --manifest-path tools/analysis-core/Cargo.toml --features analyzer-cli --release --locked --bin eska-analyzer
```

В выбранном проекте запускается собранный бинарник:

```bash
eska-analyzer --project-dir .
eska-analyzer --project-dir . --project main
```

Стандартный initialize должен явно запросить ESKA protocol `0.1`; после
initialized доступны страницы владельцев и чтение/refresh одного YAML.
Новый UUID сеанса и строковая ревизия обязательны в custom requests.
Объявлена full-document синхронизация YAML/BSL: open/change/save/close используют
общую ревизию и текущую source-byte квоту. Save не заменяет текст редактора;
close перечитывает файл. Языковые запросы, cancellation и edit plans ещё не объявлены.
Из установленного комплекта бинарник не запускается автоматически.
[Протокол, границы и проверка C1/C2/C3](../../docs/research/eska-format/analyzer-metadata.md).
[Синхронизация документов и события состояния](../../docs/research/eska-format/analyzer-documents.md).
