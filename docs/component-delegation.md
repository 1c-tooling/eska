# Делегирование общей команды

T96 выполняется по частям. Сейчас `eska` запускает отдельный процесс владельца:

| Команды общего CLI | Процесс |
|---|---|
| Без команды, `metadata`, `version`, `ide` | `eska-project` |
| `clean`, `platform`, `config` | `eska-build` |
| `clone`, `status`, `diff`, `history`, `start`, `switch`, `save`, `finish`, `shelve`, `unshelve`, `shelves` | `eska-vcs` |

`build`, `new/init`, `new --from`, `import`, `patch`, `doctor` пока используют
прежние адаптеры общей команды. Их процессные клиенты provenance,
preflight и транзакций ещё не завершены; T96 остаётся `IN PROGRESS`. `update` сохраняет прежнего
владельца до выборочного установщика T97. Состав зависимостей root package
пока не является окончательным составом лёгкой оболочки.

## Поиск компонента

Для каждого инструмента можно явно задать полный или относительный путь:
`ESKA_PROJECT_BINARY`, `ESKA_BUILD_BINARY`, `ESKA_VCS_BINARY`. Явный неверный
путь даёт ошибку; другой установленный бинарник его не заменяет.
Без override порядок такой: рядом с реально запущенной `eska`, затем PATH,
затем `~/.eska/bin` (USERPROFILE на Windows). Имена учитывают `.exe`.
Каталог и сама оболочка не принимаются за исполняемый файл компонента.
Выбранный файл должен разрешаться существующей файловой системой.

Если компонент отсутствует, операция завершается до проектных изменений.
Вывод содержит имя инструмента и переменную его пути; автоматическая установка
не запускается. Справка и версия самой оболочки доступны без компонентов.
Согласование смешанных версий и manifests установки завершается в T97.

## Потоки и отмена

Оболочка передаёт исходные аргументы без shell quoting или UTF-8 преобразования.
`stdin`, `stdout`, `stderr` наследуются напрямую; оболочка не читает протокол,
не буферизует native output и не добавляет свои сообщения в stdout компонента.
Процесс остаётся в той же группе терминала, поэтому TTY и интерактивный ввод
сохраняются. Возвращается exit code дочернего процесса; Unix signal termination
представляется как 128 + signal. На Windows сохранение полного exit status
использует stable `process::exit` для значений вне u8 после завершения ребёнка.

На Unix SIGINT/SIGTERM/SIGHUP регистрируются через `signal-hook` 0.4.5
с отключёнными default features. Передача точному owned PID использует
закреплённый `nix` 0.31.3 с `signal/process`; stable std не предоставляет
эквивалент отправки произвольного сигнала. Handler меняет только atomic flag,
обычный код передаёт сигнал и ждёт cleanup компонента. Через 10 секунд
не завершившийся процесс принудительно завершается и reaped. Ошибка wait/signal
также не оставляет принадлежащий оболочке прямой child.
Процессы по имени не ищутся и не завершаются.

На Windows оба процесса прикреплены к одной консоли; `ctrlc` принимает событие,
предусмотрено такое же ограниченное ожидание с forced fallback. Программная
адресная доставка console event отдельному child и его native descendants
ещё не проверена. Windows-проверка не объявлена пройденной.

Контракт проверен по [std Child](https://doc.rust-lang.org/std/process/struct.Child.html),
[signal-hook register_usize](https://docs.rs/signal-hook/0.4.5/signal_hook/flag/fn.register_usize.html)
и [nix kill](https://docs.rs/nix/0.31.3/nix/sys/signal/fn.kill.html).

## Старые входы проекта

`eska ide --stdio` становится `eska-project serve --legacy-identity --stdio`.
Скрытый compatibility flag выбирает прежние identity `eska` и capabilities;
обычный `eska-project serve` продолжает объявлять `eska-project`.
Сервер и lifecycle остаются одной реализацией, второй proxy не создан.
Завершение stdin останавливает сам сервер, оболочка ждёт и возвращает его код.

Запуск `eska` без команды сохраняет тихую проверку manifest/discovery через
скрытый `eska-project __shell-validate`. Он не подменяется новым standalone
validate с полным выводом и проверкой XML. Это внутренние входы композиции,
в обычной справке проектного инструмента они не появляются.

## Промежуточные проверки

Workspace strict Clippy и сборка всех бинарников прошли. 534 теста общего CLI
и project-компонента passed, 7 ignored, включая настоящие API-сессии и прежние
workflow/semantic регрессии, теперь идущие через отдельный процесс.
Новые process-тесты подтвердили точные бинарные streams, native аргументы,
exit 197, identity rewrite при `--project-dir ide`, отсутствие fallback после
неверного override и передачу трёх Unix-сигналов с reaping фактического PID.
Нативные составные операции и полный финальный workspace-набор остаются
следующими проверками T96.

## Протокол VCS для составных операций

`eska-vcs --project-dir ROOT __bridge provenance` возвращает schema 1,
operation `provenance`, `data` с прежними status/commit/dirty и `error: null`.
`__bridge source-changes --files N [--empty-scaffold]` возвращает operation
`source_changes` и `data.local_changes`. Это скрытые read-only входы для
композиции. Параметры files/empty-scaffold передаёт владелец уже проверенного
source-плана; они не являются самостоятельной проверкой его содержимого.

Git-less source сохраняет прежнее правило подтверждения (N либо 0 для
проверенного scaffold). Повреждённый Git даёт `data: null`, `error: "repository"`
и exit 1 при source preflight. Provenance в том же случае сообщает unavailable;
это не доказанная чистота. Исходники, HEAD и raw index запросы не меняют.
Пути передаются через native аргументы, protocol JSON их не перекодирует.

`GitProvenance` теперь находится в общей библиотеке как чистые данные без
repository runtime. Старые поля/значения паспорта сохраняются, десериализация
отклоняет несогласованные status/commit/dirty, неизвестные поля и неверные IDs.
Вызов самой VCS-реализации в оставшихся native-адаптерах пока библиотечный;
переход на этот process-контракт — следующая часть T96.

Проверки этой границы: workspace strict Clippy и 275 тестов core/VCS/build
runtime passed, 1 ignored. Два новых настоящих process-теста подтвердили scope
source, локальную историю, byte-read-only HEAD/index/source, обе локали,
пустой PATH, отличие absent от broken Git и отказ вместо ложного нуля.
