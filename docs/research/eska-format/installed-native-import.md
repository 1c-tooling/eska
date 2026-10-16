# Native-импорт при открытом Explorer

Linux-приёмка добавления объекта прошла для CF/CFE/EPF/ERF: установленный VSIX,
настоящие `eska import` preview и exact-plan write, платформа 8.3.27.2325.
Incoming artifacts и исходные XML взяты из сохранённой native-матрицы;
проверка работает только с собственными копиями.

## Смена сессии

Открытые изменённые YAML/BSL и объявленные SDBL сохраняют текст, editor version
и wire version. После исчезновения import guard клиент запускает новый host
с прежним independently configured pin. `ready` публикуется после полного ACK
сохранённых документов. Прежний child не возвращает актуальный metadata result.

Удаление произвольного файла больше не запускает отдельное переподключение.
Mixed batch сохраняет recovery hint только для точных объявленных import,
manifest и root controls; settings и неизвестные изменения его не получают.

Запоздалое событие marker после успешного импорта могло завершить уже новый host.
Перед автоматическим запуском клиент теперь фиксирует отсутствие marker,
bounded opaque bytes и identity history, а также metadata каталога проекта.
Последнее обнаруживает и новую операцию с rollback, не изменившим history.
JSON history не разбирается для принятия source, settings или прав поддержки.

Только точное событие этого marker может оказаться устаревшим. Активный guard,
изменённая generation, aliases или ошибка наблюдения не дают его подавить.
Stamp связан с конкретным target и одной регистрацией нового процесса;
проверки после `await` не позволяют старому watcher отправить событие другому
child. Root, manifest и settings остаются на обычном пути SDK. Его protocol
и необратимое завершение старого host не менялись.

Устаревший ответ location при открытии вкладки отменяет это чтение без ложной
ошибки в UI. Остальные ошибки открытия по-прежнему показываются пользователю.

## Проверенный сценарий

| Тип | Dirty-вкладки | Новый состав | Обычных Save после импорта |
|---|---|---|---|
| CF | YAML, BSL, SDBL | Owner и модуль | 3 |
| CFE | YAML, BSL, SDBL | Owner и модуль | 3 |
| EPF | YAML, BSL | Форма и модуль | 2 |
| ERF | YAML, BSL, SDBL | Шаблон, ресурс и запрос | 3 |

У native EPF этого набора нет SDBL; дополнительный synthetic query не создавался.
Новые bindings, bytes модулей/ресурса/запроса сверяются с независимым native dump.
Receipt, transaction, ancestor, published state и отсутствие guard проверяются
по сохранённым файлам. У каждого типа подтверждён ровно один переход
`restart → connecting → ready`, стабильный через открытие новых файлов и Save.
Accepted storage, pin, settings и оригиналы входов сохранены.

Save не зависит от property preview: в LocalSyntax CFE/EPF/ERF этот preview
отключён. YAML меняется обычной editor transaction с сохранением допустимой
CST-границы значения; пустое значение получает текст с прежними отступом и
переносом строки.
Эквивалентные произвольные lexical spellings этой частью не принимаются.
Все 11 публикаций проходят production provider и настоящий mutating SDK.

[Обезличенные результаты](installed-native-import-results.json) содержат gates,
проверку текущих inputs и VSIX, installed regressions и завершение собственных
процессов. Raw DTO, пути, UUID и source hashes остаются в private receipts.
Изменение клиента зафиксировано в `eska-vscode-explorer` commit `9f01412`.
Rust production inputs не изменены; прежний проверенный комплект CLI используется
с повторной сверкой hashes. Новые постоянные ИБ не оставлены; четыре пользовательские
базы сохранены с прежними размером, mtime и inode.

Это приёмка native addition. Installed rename/delete, orphan drafts, настоящее
прерывание с `Complete`/`RollBack`, workspace-матрица и общий workflow EF-20
остаются открыты. Build/dump и выполнение модулей/запросов здесь не повторяются;
physical human typing и полная EF-16/MVP приёмка не заявляются.
