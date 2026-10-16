# Анализатор во время повторного импорта

EF-25 добавляет guard выбранного проекта в control inventory
`eska-analyzer`. URI `.eska-project-source-import.json` объявляется с
`kind: source_import`; standalone получает 7 control watchers, workspace member —
8. Root descriptor выбирается из текущей карты, поэтому configuration, extension,
external processing и external report используют собственный путь.

## Граница сессии

Новый host повторно проверяет guard после открытия reader и после выделения
analysis session, до ответа `initialize`. Незавершённый импорт даёт
`source_import_incomplete`. Любой существующий filesystem entry по guard path,
включая каталог и dangling symlink, блокирует открытие; receipt не разбирается
для принятия решения.

Работающий сервер проверяет этот путь перед проектным запросом, full-document
input и обработкой валидного saved-file batch. Проверка также выполняется после
dispatch и после сериализации, перед записью frame. Поэтому импорт, начавшийся
во время вычисления, не публикует уже вычисленный результат. Ошибка наблюдения
пути на финальной границе также завершает host; её machine cause сохраняется
в restart state.

Наблюдение guard освобождает analysis host, документы, settings, edit plans и
watcher registration. RPC получает `project_restart_required`, document input —
`eska/inputRejected`, file batch — `eska/restartRequired`. Старый host остаётся
завершённым после удаления guard, завершения импорта или rollback.

Если первый file batch содержит только удалённый root descriptor или
`source.json`, SDK добавляет наблюдаемый `source_import` к control changes.
Отдельного watcher event для guard ждать не нужно. Одновременные изменения
project/analysis settings сохраняются: наличие guard не скрывает причину,
которая требует явного перезапуска с новыми настройками.

Restart state содержит snapshot последнего полностью отправленного frame.
Внутренний результат lazy semantic load и обработанная, но ещё не отправленная
версия документа не становятся подтверждённым вводом клиента. Исходный request
ID и имя отклонённой document operation сохраняются.

## Несохранённые документы

Текст, version и URI открытого документа остаются у клиента. Сервер не сохраняет
буфер при импорте и не переносит его на другой owner автоматически. После
завершения/recovery клиент запускает новый процесс и явно повторяет актуальные
документы. При rename он должен выбрать новый URI; удалённый source возвращает
`unknown_source`. Client может оставить такой документ открытым для ручного
разрешения. BOM, CRLF, comments и non-BMP characters не нормализуются.

Это соответствует [LSP didOpen](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/textDocument/didOpen.md):
в открытом документе клиент управляет текстом. События изменения состава по-прежнему
доставляет [client watcher](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/workspace/didChangeWatchedFiles.md).

Проверка guard не является cross-process lock. Импорт, целиком прошедший между
двумя проверками, обнаруживается через обязательные control file events.
[Native addition в установленном Explorer](installed-native-import.md) теперь
проверяет автоматический restart, сохранение буферов и replay для четырёх типов.
Server transport не заменяет остальные клиентские сценарии EF-25/EF-11.

## Проверки

Process matrix на RU/EN проверяет query/document/saved-event triggers без события
guard, file/directory/dangling-symlink marker, холодный отказ и неизменность диска.
Дополнительные RU/EN cases действительно перемещают `src` и передают только
root/manifest deletion batch; оба control changes и import hint сохраняются.
Отдельно проверены project/analysis settings вместе с guard. Полный SDK gate —
264 default / 601 all-features tests, strict Clippy и Rustdoc; все 17 root/SDK
проверок прошли. Это process-проверки, installed interruption описывается отдельно.
Реальный source-only journal меняет native composition; обе recovery actions
проверены после удаления incoming store. Старый host остаётся заблокированным,
новый видит актуальный состав и принимает exact dirty replay по выбранному URI.

Два deterministic worker cases ставят guard между настоящим dispatch и wire
publication: semantic load со сменой revision и document v8 после подтверждённой
v7. Проверяются отказ stale result/ack, last published snapshot, исходная operation
и exact replay hash на обеих локалях. Production test hooks не добавлены.

Linux probe прошёл настоящий CF preview/write через `eska-build` и 1С
8.3.27.2325 с уже открытыми dirty YAML/BSL. Guard наблюдался во время собственного
CLI импорта; query без watcher event завершил прежний host. После публикации
новый процесс увидел удалённый owner и отклонил orphan URI, сохранив caller text.
Settings, original support pin и предыдущий CF сохранились.
[Обезличенные результаты](analyzer-import-guard-results.json).

Эта server-матрица не принимала native CFE/EPF/ERF и реальный OS/client watcher.
Последующая [installed-матрица](installed-native-import.md) приняла all4 addition
с автоматическим восстановлением вкладок и последующим Save. Последующая
[rename/delete-матрица](installed-native-retirement.md) приняла all4 и orphan drafts.
Следующая [installed recovery-матрица](installed-native-recovery.md) приняла
настоящую границу `BackedUp` и обе recovery actions для all4 standalone.
Workspace и остальные точки прерывания остаются открыты; EF-25 — `IN_PROGRESS`,
MVP — в работе.
