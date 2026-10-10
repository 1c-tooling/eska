# Уведомления об изменениях сохранённых файлов

Здесь сохранён компонент на `8988627`. Следующая
[защита после изменений состава](analyzer-project-changes.md) добавила
`eska/restartRequired`; counts и замеры ниже относятся к saved file events.

`eska-analyzer` принимает `workspace/didChangeWatchedFiles` для объявленных
YAML и BSL выбранного проекта. Событие очищает прежний saved input и его
каноническую проекцию, переводит его в pending и освобождает source-byte
reservation. Файл читается при следующем metadata read или языковом запросе.
Уведомление не читает диск, не сканирует каталог и не добавляет индекс.

Правило живёт в общей `AnalysisSession::invalidate_saved_inputs`.
`MetadataInputs::invalidate_saved` использует прежний controller, публикация
YAML очищает тот же Salsa slot, BSL — прежний tracked unread state. Transport
проверяет параметры и показывает результат. Retained snapshot остаётся
неизменным, но больше не служит текущей целью навигации или диагностики.

## Контракт

После initialize/initialized клиент передаёт `changes`: массив объектов
`uri` и `type`. Типы соответствуют
[LSP 3.18](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/workspace/didChangeWatchedFiles.md):
1 — создание, 2 — изменение, 3 — удаление. Все три типа требуют повторной оценки
текущих bytes; само событие не доказывает наличие или отсутствие файла.
Создание поддержано только для повторно появившегося объявленного файла.

Максимум — 200 исходных событий на пакет, включая дубликаты; пустой пакет
разрешён. URI имеют прежние local/declaration/path проверки, без query/fragment.
Неверный type, неописанное поле, внешний URI, неизвестный source или binary
module отклоняют весь пакет до изменения любого входа. Ошибки остаются
`eska/inputRejected`, operation — workspace/didChangeWatchedFiles.

Дубликаты объединяются по SourcePath. Весь пакет, включая возможность увеличить
revision, проверяется перед публикацией. Обработка идёт в порядке SourcePath.
Каждый saved input, который ещё не pending, получает новую общую revision;
уже pending входы и overlays не меняются. Открытый YAML/BSL сохраняет bytes, hash и document
version даже при удалении файла на диске. Close читает реальные saved bytes,
включая unavailable. Created сбрасывает прежнее unavailable в pending;
следующий read может восстановить ready.

Один `eska/inputsChanged` подтверждает весь пакет. Params содержат `snapshot`,
`inputs` и `resources`. Каждый input — `source`, `invalidated` и прежняя
форма `input` из [document sync](analyzer-documents.md). Массив отсортирован,
дубликатов нет. `invalidated=false` означает overlay или уже pending; это
не утверждение об отсутствии изменений на диске. При invalidated=true hash
сбрасывается в null. В ответе нет текста YAML/BSL.

Initialize сообщает experimental.eska.watchedFiles: method,
scope=declared-saved-inputs, inputNotification=eska/inputsChanged,
maxChanges=200, lazyRead=true, dynamicRegistration=false,
compositionUpdates=false. Новый event добавлен в inputNotifications.
Обычный request с этим методом не заменяет notification.

Клиент пока сам настраивает наблюдение и отправляет события. Динамическая
LSP-регистрация и OS watcher не реализованы. Состав `source.json`, корневые
ссылки configuration и settings остаются снимком открытия: после их изменения
нужно перезапустить сервер. Добавление новых owners/modules не разрешается
событием Created. Config/manifest events не вызывают автоматический
restart-required переход; это следующий компонент. Клиенту нужно разделять
события файлов ввода и изменения состава проекта.

## Проверки

Прошли 51 unit/file, 52 analyzer process и 9 checker process tests, все 19 tool
gates четырёх feature profiles, strict Clippy, Rustdoc и release. Root: 893 passed /
14 прежних ignored; minimal 165/3, project-cli 181/3, build-cli 200/3, все 15 gates
прошли. Проверены preflight без частичных обновлений, clock exhaustion,
старая revision, duplicate/pending, бинарные входы, удаление, восстановление,
общий source budget и сохранение YAML/BSL editor authority.

Для C1/C2/C3 в RU/EN прошли шесть сценариев: 30 подтверждённых definition,
42 locate, 42 definition с отказами и 30 принятых уведомлений. Состав каждого
реального проекта сохранён целиком; в собственный временный каталог скопированы
только configuration, manifest, выбранные catalog/common-module owners и caller.
Остальные payloads намеренно отсутствуют: это ограниченная проверка с неполным
вводом. Saved caller и editor-вариант — контролируемые маленькие BSL-фрагменты.

Прежние YAML-цели исчезают сразу после события; языковой запрос может прочитать
только свой BSL caller и не подменяет явное чтение pending YAML. После read
точные UUID, UTF-16/byte ranges и hashes совпадают с текущими saved bytes.
Delete/read возвращает unavailable, Created/read — ready. Неверный смешанный
пакет сохраняет обе готовые модели. Editor и close согласованы с деревом,
навигацией и синтаксической диагностикой. Machine traces совпали между локалями.

| Corpus | Уведомления, мс | Peak RSS, KiB |
|---|---:|---:|
| C1 | 0,133–0,596 | 77936–78108 |
| C2 | 0,340–1,446 | 218696–218736 |
| C3 | 0,220–0,673 | 245064–245520 |

Это small-batch замеры, без гарантии задержки или общего heap/RSS budget.
Пять выбранных оригинальных файлов каждого сценария сохранили bytes;
оригинальные corpus files не записывались. Копии восстановлены, собственные
временные проекты удалены. PATH пустой, normal exit не ждал stdin EOF.
Платформа 1С, настоящий VS Code client и OS watcher не запускались.

Runtime graph остался 206 package names / 214 resolved versions;
release server — 48878760 bytes. [Машинный отчёт](analyzer-watched-files-results.json)
содержит gates и замеры без имён конфигураций и локальных путей.
EF-18 остаётся IN_PROGRESS: полнота моделей/семантики, все IDE edits,
общий memory budget, наблюдение за составом и подключение Explorer ещё в работе.
