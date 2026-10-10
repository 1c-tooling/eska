# Регистрация наблюдения за файлами состава

`eska-analyzer` запрашивает у клиента одну динамическую LSP-подписку на точные
файлы состава и настроек. Область — `project-control-inputs`: шесть путей
standalone-проекта или семь для workspace member. Состав конфигурации не
увеличивает число подписок. Сохранённые YAML/BSL по-прежнему используют
[явные client file events](analyzer-watched-files.md); автоматическое наблюдение
за всеми их путями ещё не подключено.

## Согласование с клиентом

Оба flags `workspace.didChangeWatchedFiles.dynamicRegistration` и
`relativePatternSupport` должны быть true. При отсутствии любого из них сервер
сохраняет ручной список control URI и не отправляет запрос регистрации.
Правила соответствуют текущим первичным спецификациям
[file events](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/workspace/didChangeWatchedFiles.md),
[регистрации](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_includes/messages/3.18/registerCapability.md)
и [relative patterns](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/types/patterns.md).

После [initialized](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_includes/messages/3.18/initialized.md)
сервер отправляет один `client/registerCapability`, request ID —
`eska/project-watch/1`. Массив registrations содержит одну регистрацию
`eska/project-control-files` для `workspace/didChangeWatchedFiles`.
Каждый watcher имеет `globPattern={baseUri,pattern}` и `kind=7` для
Create/Change/Delete. Pattern — точное стандартное имя файла; parent directory
представлен URI. Unicode, пробелы и glob characters в пути проекта не
превращаются в wildcard. Регистрируются также отсутствующие кандидаты settings.

Initialize добавляет `experimental.eska.watchRegistration`: method,
scope=project-control-inputs, eligible, requiresRelativePatterns=true и
inputNotification=eska/watchRegistration. Eligible означает возможность
отправить запрос, а не установленную подписку. Существующий watchedFiles
с scope=declared-saved-inputs сохраняет dynamicRegistration=false.
Это два явно разных охвата, статической LSP-подписки нет.

## Ответ и жизненный цикл

Один serial worker принимает client replies через прежнюю ограниченную очередь.
Ответ не становится входящим request и не получает ответного JSON-RPC message.
Числовые, строковые и null response IDs разбираются отдельно от request IDs;
null request ID по-прежнему отклоняется. Формат ответа требует ровно result
или структурированный error, без batch, дубликатов keys и лишних полей.

Для ожидаемого ID `result:null` подтверждает регистрацию. Сервер отправляет
`eska/watchRegistration`: scope, текущий snapshot, registrationId и
status=registered. Error клиента даёт status=failed,
error.kind=client_rejected_registration и исходный numeric code.
Непустой result даёт registration_result_invalid с code=null. Произвольные
client message/data не возвращаются в событии. Все machine fields одинаковы
между локалями; подтверждение не увеличивает revision и не читает файлы.

Запрос регистрации не блокирует metadata или BSL work. В памяти может оставаться
только одна ожидаемая регистрация; автоматических retries и таймера ответа нет.
Без подтверждения клиент не должен считать наблюдение готовым. После отказа
он может передавать события самостоятельно либо перезапустить соединение.
Неизвестный, повторный, преждевременный или запоздалый reply игнорируется.
Client request с таким же ID сохраняет собственную независимую identity.

[Control event](analyzer-project-changes.md) завершает прежний host и очищает
ожидание регистрации; поздний успешный reply не снимает restart-required.
Shutdown не ждёт ответа клиента. Подписка принадлежит соединению клиента;
сервер не запускает OS watcher, не отправляет unregister и не управляет
автоматическим перезапуском клиента.

## Проверки

Прошли 53 unit/file, 59 analyzer process и 9 checker process tests, все 19 tool
gates четырёх feature profiles, strict Clippy, Rustdoc и release. Root: 893 passed /
14 прежних ignored; minimal 165/3, project-cli 181/3, build-cli 200/3, все 15 gates
прошли. Проверены capability matrix, Unicode/glob directories, точное совпадение
control inventory, регистрация после initialized только один раз, независимость
request/reply IDs, refusal/invalid result, late replies и shutdown без ответа.

На C1/C2/C3 в RU/EN прошли 42 control-event сценария с подтверждённой
регистрацией: 84 definition, 84 независимых locate, 336 запросов к завершённой
сессии и 210 отклонённых input notifications. Семь точных watchers соответствуют
объявленному control inventory. Новый процесс получает новую регистрацию
и session UUID. Пройдены также шесть прежних saved-input сценариев с шестью
control watchers: 30 подтверждённых definition, 42 locate, 42 definition
с отказами и 30 принятых file notifications.

Сценарии выполнялись на собственных частичных payload copies с полным составом
реальных проектов. Остальные payloads намеренно отсутствовали; semantic complete
остаётся false. Точные current UTF-16/byte ranges, hashes, editor authority и
locale traces проверены. Оригинальные corpus files не записывались; пять
выбранных файлов каждого сценария сохранили bytes. Копии восстановлены,
собственные временные проекты удалены. PATH пустой, normal exit не ждал stdin EOF.

Размер запроса регистрации в этих путях — 1157 bytes для standalone и 1365
для workspace member. Он зависит от длины parent URI, не от числа owners/modules.
Это протокольная проверка: настоящий VS Code client, OS watcher и платформа 1С
не запускались; зарегистрированный result не доказывает доставку событий ОС.
Замеры не являются SLA или общим heap/RSS budget.

Runtime graph совпал с предыдущим компонентом: 206 package names / 214 resolved
versions, без Git engine, LSP runtime или сетевого клиента. Существующий отдельный
gix-validate остался только вспомогательной зависимостью проверки путей.
Release server — 48977664 bytes. [Машинный отчёт](analyzer-watcher-registration-results.json)
содержит gates и ограниченные замеры без локальных путей или имён конфигураций.
EF-18 остаётся IN_PROGRESS: наблюдение всех saved inputs, интеграция клиента,
полная семантика/модели, все IDE edits, общий memory budget и Explorer ещё в работе.
