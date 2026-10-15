# Перезапуск после изменения состава проекта

Здесь сохранён компонент на `b5970bf`. Следующая
[регистрация control watcher](analyzer-watcher-registration.md) добавила
post-initialized LSP-подписку; counts и замеры ниже относятся к restart guard.
Позднейший [guarded Save](guarded-source-save.md) добавил узкое исключение для
подтверждённой публикации typed property выбранного root YAML; оно описано ниже.

`eska-analyzer` больше не продолжает работу по прежнему составу после события
изменения манифеста, корневой конфигурации или настроек. Он принимает
`workspace/didChangeWatchedFiles`, освобождает host и сообщает
`eska/restartRequired`. Старый процесс остаётся доступен для shutdown/exit;
проектные запросы и документные уведомления отклоняются до нового запуска.

## Граница ответственности

Сервер сохраняет небольшой неизменяемый список файлов, от которых зависит
выбранная сессия. В него входят `source.json`, `configuration.yaml`, проектный
`eska.toml`, а для workspace member — также родительский `eska.toml`.
Три кандидата настроек анализа берутся из закреплённого
[ProjectConfig](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/project-model/src/lib.rs):
`bsl-analyzer.toml`, `.bsl-analyzer.json` и `.bsl-language-server.json`.
Создание прежде отсутствующего кандидата также требует перезапуска.
`.env` не добавлен: текущий consumer не использует resolver окружения и зависимостей.

В исходном компоненте transport сопоставлял точные local paths без чтения
изменённых файлов, сканирования каталогов, второго индекса или повторного импорта XML.
Корневая `configuration.yaml` относилась к control inputs целиком: даже изменение
отдельного её свойства консервативно требовало перезапуска. Структурность изменения
по уже изменённому содержимому не определялась.

Текущий Save сохраняет private fingerprint только после `published` typed-property
изменения выбранного root. Совпадение пути и безопасно прочитанных полных bytes
переводит его file event в обычный saved/Overlay watch, в том числе при объединённых
delete/create. Это не общий разбор изменённого root: `already_current` не создаёт
fingerprint, а missing/alias/другие bytes требуют прежнего restart. Foreign root,
source manifest, project/workspace/analysis settings и import не исключаются.
Состав и accepted-base pin не обновляются; `ReadPermissions` принимает новый hash
только соответствующего выбранного root. Итоговые проверки этого среза фиксируются отдельно
от исторических counts ниже.

Наблюдение принадлежит клиенту; OS watcher и динамическая LSP-регистрация пока
не подключены. Сервер сообщает точный список URI, чтобы клиент не угадывал
workspace boundary. Автоматическая пересборка состава и перезапуск процесса
также не реализованы.

## Протокол

ESKA protocol остаётся `0.1`. Initialize добавляет
`experimental.eska.projectChanges`: `method`, `policy=restart`,
`notification=eska/restartRequired`, `inputs`. Каждый input содержит `kind`
и local `uri`; kinds — `source_manifest`, `root_configuration`,
`project_settings`, `workspace_settings`, `analysis_settings`.
`eska/restartRequired` добавлен в `inputNotifications`.

Пакет имеет прежний лимит 200 исходных событий. Все поля, event types 1/2/3,
URI без query/fragment и границы путей проверяются до изменения состояния.
Malformed event или внешний путь отклоняет весь пакет через `eska/inputRejected`.
Точный control path разрешён и за пределами source root. Дубликаты объединяются
по пути, итоговые control changes отсортированы.

При наличии control change сервер завершает сессию до проверки объявленности
остальных путей внутри source. Поэтому одновременное изменение `source.json`
и создание нового owner не получает ложный отказ «необъявленный source»:
новый файл не регистрируется и не читается, старые входы не обновляются.
Чистый пакет YAML/BSL сохраняет прежние declaration/binary проверки и
[инвалидацию сохранённых входов](analyzer-watched-files.md).

Один `eska/restartRequired` содержит `snapshot` и `changes` — массив `kind/uri`.
Revision не увеличивается: snapshot обозначает последний принятый снимок,
который после уведомления больше нельзя использовать. Клиент должен отбросить
его дерево, диагностику, навигационные результаты и edit plans.
Host, settings, plans и учёт открытых документов освобождаются.

Любой последующий проектный request получает code `-32000`,
`error.data.kind=project_restart_required` и тот же state в details;
проверка идёт до params, I/O и таймера. Open/change/save/close и повторные
file events получают `eska/inputRejected` с тем же отказом. Возврат файла
к прежним bytes не снимает это состояние. Для нового процесса нужен обычный
initialize/initialized с новым session UUID; клиент затем повторно передаёт
свои открытые документы. Сервер не записывает и не заменяет их текст на диске.

## Проверки

Прошли 51 unit/file, 55 analyzer process и 9 checker process tests, все 19 tool
gates четырёх feature profiles, strict Clippy, Rustdoc и release. Root: 893 passed /
14 прежних ignored; minimal 165/3, project-cli 181/3, build-cli 200/3, все 15 gates
прошли. Process tests проверяют каждый standalone control с каждым event type
в RU/EN, родительский workspace manifest, общий preflight и отказ старого host.

На C1/C2/C3 в RU/EN прошли 42 сценария: семь control inputs, включая workspace,
на каждый corpus и locale. Выполнены 84 подтверждённых definition и 84 независимых
locate, 336 запросов к завершённой сессии и 210 отклонённых input notifications.
Событие с control, новым source и дубликатом завершает сессию без чтения
испорченного или удалённого control file. После физического восстановления
старый host всё ещё отклоняет операции. Новый процесс получает новый UUID,
читает текущий saved ввод и подтверждает точные UTF-16/byte ranges и hashes.
Machine traces совпали между локалями.

В собственных временных workspace сохранён полный состав каждого реального
проекта, скопированы только manifest, configuration, выбранные catalog/common-module
owners и caller. Остальные payloads намеренно отсутствуют; BSL-фрагменты малы
и контролируемы. Это проверка с неполным вводом, semantic complete остаётся false.

| Corpus | Завершение сессии, мс | RSS после завершения, KiB | Peak RSS нового процесса, KiB |
|---|---:|---:|---:|
| C1 | 4,023–6,609 | 67728–68092 | 68648–69300 |
| C2 | 40,376–50,973 | 207016–207540 | 218356–219216 |
| C3 | 48,275–57,358 | 231828–232456 | 245556–246236 |

RSS не обязан вернуться к начальному значению после drop: эти замеры не задают
общий heap/RSS budget или SLA. Дополнительно пройдены шесть прежних saved-input
сценариев на текущем бинарнике: 30 подтверждённых definition, 42 locate,
42 definition с отказами и 30 принятых file notifications. Сохранены lazy read,
editor authority, атомарный отказ пакета, delete/recreate и close.

Оригинальные corpus files не записывались; пять выбранных файлов каждого
сценария сохранили bytes. Копии восстановлены, собственные временные проекты
удалены. PATH пустой; normal exit не ждал stdin EOF. Платформа 1С,
настоящий VS Code client и OS watcher в этих проверках не запускались.

Runtime graph остался 206 package names / 214 resolved versions;
release server — 48952472 bytes. [Машинный отчёт](analyzer-project-changes-results.json)
содержит gates и замеры без имён конфигураций и локальных путей.
EF-18 остаётся IN_PROGRESS: client watcher/dynamic registration, полная семантика,
остальные модели, все IDE edits, общий memory budget и Explorer ещё в работе.
