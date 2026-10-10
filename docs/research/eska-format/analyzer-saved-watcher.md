# Регистрация наблюдения за сохранёнными исходниками

Результат компонента `2c0b22f` от 2026-10-10. Последующая
[проекция процессов, задач и планов обмена](analyzer-process-objects.md)
расширяет модели и навигацию; результаты ниже относятся к этому компоненту.

После `initialized` сервер запрашивает наблюдение за файлами состава и
объявленными YAML/BSL-входами. Требуются оба true flags:
`workspace.didChangeWatchedFiles.dynamicRegistration` и `relativePatternSupport`.
Это расширение прежней [регистрации control files](analyzer-watcher-registration.md)
в экспериментальном ESKA protocol 0.1; действующий IDE protocol 1.x не менялся.

## Подписка

Один `client/registerCapability` с ID `eska/project-watch/1` содержит две
регистрации `workspace/didChangeWatchedFiles`:

- `eska/project-control-files` — прежние 6–7 точных control paths;
- `eska/project-saved-inputs` — source-relative `**/<basename>` для объявленных
  владельцев и модулей, например `**/object.yaml` и `**/Module.bsl`.

Basenames берутся из `SourceReader.owner_paths()` и `module_paths()` и
объединяются. Configuration YAML остаётся control input. Двоичные модули
включены; произвольные сохранённые ресурсы не превращаются в анализируемый ввод.
При пустом saved inventory используется безвредный `**/object.yaml`.
Подписка не перечисляет тысячи owners и не обходит файловую систему.
Имена родительских каталогов остаются в base URI, а не попадают в glob.
[RelativePattern](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/types/patterns.md)
допускает отдельный абсолютный base URI и любое число вложенных сегментов.
Всем patterns задан `kind: 7`, то есть create/change/delete.

Прежние lifecycle и request ID сохраняются: только один post-initialized запрос,
один aggregate reply, без retry, timeout или unregister. `eska/watchRegistration`
содержит прежний control `registrationId` и новый массив `registrationIds` для
обеих регистраций. `registered` означает null result клиента для этого запроса;
отказ или неизвестный/поздний reply не подтверждает установку watcher.
Host и shutdown не ждут ответа. Retirement освобождает состояние регистрации.

## Пакеты событий

Сервер объявляет согласованную политику уже при initialize, до client reply:
события могут прийти во время регистрации. `watchedFiles.dynamicRegistration`
равен true, `maxChanges` — null; предел UTF-8 request frame остаётся 4194304 bytes.
Сервер не делит слишком большой клиентский frame. Клиент должен ограничивать
свои пакеты; response frame по-прежнему ограничен 64 MiB.

`unknownSourceEvents: "ignore"` разрешает посторонние совпадения recursive glob
только внутри выбранного source. После полного preflight объявленные пути
объединяются и передаются прежней общей инвалидации. `eska/inputsChanged` добавляет
`ignoredSources`: число уникальных проигнорированных путей данного пакета.
Полностью посторонний допустимый пакет не меняет revision и resources.

`binaryInputs: "acknowledge"` сохраняет у двоичного модуля состояние binary и
`invalidated: false`: анализируемого текста у него нет. Это не подтверждение
байтов файла или успешного анализа. Такой модуль не отклоняет изменения соседних
YAML/BSL. Editor inputs и уже pending saved inputs также не инвалидируются.
Новая композиция или смена вида модуля требует изменения control input и нового host.

Внешний/некорректный URI, неверный тип события и остальные ошибки пути отклоняют
весь пакет до изменения входов. Валидное control событие сохраняет прежний
приоритет и требует нового процесса без чтения изменённого файла.
Accepted file events не читают payloads; они очищают прежние saved модели,
хеши и диапазоны, последующее чтение остаётся явным и ленивым.

Клиент без двух watcher flags сохраняет прежнюю политику: максимум 200 событий,
отказ для undeclared и binary inputs. Дополнительные capabilities
`maxRequestBytes`, `unknownSourceEvents`, `binaryInputs` и поле `ignoredSources`
документированы для обеих политик. Отказ регистрации не переключает объявленную
политику: частичная регистрация или уже доставляемые события не должны теряться.

## Проверки

Прошли 59 unit/file, 66 analyzer process и 9 checker process tests; все 19 tool
gates для четырёх профилей и 15 root gates. Приложение: 893 passed / 14 прежних
ignored; minimal 165/3, project-cli 181/3, build-cli 200/3. Binary batch regression
проверяет сохранение нечитаемого состояния вместе с успешной инвалидацией YAML.
Manual binary rejection сохранён прежним тестом. Locale traces совпали.

Независимый Rust consumer SourceReader получил состав C1/C2/C3. В нём 2644/16285/18751
saved owners и 1975/19170/22710 modules, из них 0/14/43 binary. Подписка использует
11/13/15 saved patterns. Регистрации проверены по этому составу; directory scan
и чтение module bodies для их построения не нужны.

На собственных частичных payload copies с полным составом проектов прошли
54 сценария в RU/EN: 42 control, шесть сохранения и шесть полного потока file events.
Проверены 114 подтверждённых definition, 126 независимых locate, 336 запросов
к завершённому host и 210 отклонённых input notifications. Saved/editor/close,
точные UTF-16/byte ranges, hashes и locale parity сохраняются.

Полный поток C1/C2/C3 содержит 5080/35916/41922 events на locale, включая дубликаты
и 251 посторонний путь. Собственный probe разделил поток на 3/18/21 пакетов
до 2000 событий. Все объявленные saved paths подтверждены; бинарные совпадения
не отклонили текстовые изменения. Единственный готовый YAML инвалидирован,
открытый BSL сохранён, остальные declared inputs остались pending/binary.
Удалённые payloads не читались; последующие read/close дали source_file_missing.

Максимальный проверенный request — 909868 bytes; неделённые потоки больших
проектов превышают 4 MiB. Ответы на отдельный пакет заняли 16–65 ms, peak RSS
процесса в этих потоках — 61852–213584 KiB. Это ограниченные локальные замеры,
не SLA или общий memory budget. PATH был пустой; normal exit не ждал stdin EOF.
Оригиналы не записывались: bytes пяти выбранных файлов каждого сценария проверены.
Собственные copies восстановлены и временные проекты удалены; прочие payloads
намеренно отсутствовали, semantic complete остаётся false.

Runtime graph совпал с предыдущим компонентом: 206 package names / 214 resolved
versions, без Git engine, LSP runtime или network client; gix-validate остаётся
прежним path helper. Release server — 49101344 bytes.
[Машинный отчёт](analyzer-saved-watcher-results.json) содержит gates и обезличенные замеры.

## Ограничения

Проверяется stdio protocol с подтверждением probe client. Настоящий VS Code
client, доставка OS events и автоматический restart расширения не приняты.
Нет фонового обхода проекта, live composition updates, второго индекса,
чтения XML, нового inference или dependencies.
Лимит frame и квота исходных bytes не являются общим heap/RSS budget.
EF-18 остаётся `IN_PROGRESS`: client integration, полные модели/семантика,
все IDE edits, общий memory budget и Explorer ещё требуют реализации.
