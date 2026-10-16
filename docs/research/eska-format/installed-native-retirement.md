# Удалённые и переименованные исходники при открытом Explorer

EF-25 проверяет настоящий native import CF/CFE/EPF/ERF в установленном VSIX
на Linux и 1С 8.3.27.2325. Для каждого типа отдельно используются incoming
с удалением и переименованием объекта. Исходные XML, native artifacts,
accepted storage, settings и independently configured pin сохраняются.
Проверки создают только собственные копии в соседнем playground.

## Вкладки и новый состав

До CLI preview/write открываются dirty YAML/BSL и объявленные SDBL:
независимые сохранившиеся пути и два исчезающих источника. У CF/CFE/EPF
исчезают owner YAML и BSL; у ERF — template YAML и SDBL. Native EPF этого
набора не содержит отдельного запроса.

После импорта новый host получает текущие declarations. Независимые черновики
полностью replay-ACK и проходят обычный Save. Исчезнувший источник остаётся
открытым dirty `TextDocument` с прежними текстом, editor version, wire version
и URI. Он отсутствует в новом дереве, не отправляется новому child,
не получает ACK или Save authority. Отдельные текущие источники продолжают
работать в `ready`; старый черновик не становится их контекстом.

Обычная workbench Save-команда проверяется и для orphan. Если редактор
останавливает её до provider из-за отсутствия файла или разрешения, это
отмечается отдельно. Дополнительный rejection-only вызов настоящего FSP
проверяет `NoPermissions`; он не объявляется обычным Save. Ни один путь
не достигает mutating SDK и не пересоздаёт исчезнувший файл.

При rename старый dirty URI сохраняется. Новый URI открывает saved bytes
из нового состава, а не переносит текст прежнего редактора. UUID владельца
до и после rename совпадает. Новые owner/module/query/resource bindings
и raw bytes сверяются с независимым native dump. Для EPF/ERF в собственной
копии control нормализуются только физические root filenames, как при native
reimport; XML и opaque bytes не переписываются.

Receipt, completed journal, ancestor и отсутствие guard проверяются по файлам.
Каждый import требует ровно один `restart → connecting → ready`, стабильный
через открытие новых bindings и Save. Используется прежнее ограничение
запоздалых marker events из [addition-приёмки](installed-native-import.md).

## Черновик принадлежит объекту

Два разрешённых переименования `A → C`, затем `B → A` могут занять прежний
URI другим UUID под тем же accepted pin. Source-only core probe подтвердил
оба импорта без конфликтов, сохранение pin и полностью одинаковые исходные
и текущие bytes модулей. Поэтому одного raw hash недостаточно для записи
удерживаемого черновика по прежнему пути.

Клиент теперь отдельно хранит current declaration binding и первую доказанную
привязку открытого документа. Для selected inputs это source root, UUID корня,
тип проекта, UUID владельца, язык и относительный native/query slot. Полные
изменяемые имена, revision и support pin не заменяют identity.

Bindings строятся после полного owner paging. Повторные `document`,
declaration и reconnect не меняют привязку удерживаемого документа; её
освобождает actual editor close. Несовпадение запрещает replay, ACK, read/edit
request и Save этого draft. Независимые current inputs остаются доступны.

Регрессии проверяют смену owner/root/slot, YAML/BSL/SDBL/XML, независимый
context, обычное переподключение и close/reopen. До исправления три проверки
подмены падали, положительный reconnect проходил. Dirty guarded editor
сохраняет journal authority при одновременно открытом readonly alias.

## Результаты и границы

Все 12 случаев addition/deletion/rename прошли: 33 обычных Save независимых
источников и 16 сохранённых orphan drafts. Во всех 16 попытках orphan Save
редактор остановил запись до FSP; отдельные rejection-only проверки FSP также
отказали. Восемь новых URI rename открылись с прежними UUID владельцев.
Клиентские compile/typecheck/package и все 425 Node-тестов прошли;
установленные регрессии Save/native matching также успешны. Все 58 наблюдавшихся
собственных CLI/analyzer процессов завершились без вмешательства. Четыре
пользовательские ИБ сохранили размер, mtime и inode; постоянные ИБ не созданы.
Изменение клиента зафиксировано в `eska-vscode-explorer` commit `fa8bdb9`.

[Обезличенные результаты](installed-native-retirement-results.json) содержат
актуальные gates, installed matrices, отдельный подсчёт orphan Save/FSP,
проверку VSIX, исходных файлов, пользовательских ИБ и завершения процессов.
Raw DTO, paths, UUID, source hashes и process identities остаются в private
receipts. Rust production inputs не менялись; прежний комплект CLI повторно
сверяется по hashes. Workspace MCP недоступен; установленный `bsl-analyzer-app`
проверил 12 BSL-модулей без SyntaxError и failed files. После штатного удаления
тестовых проектов входы диагностики воспроизведены из сохранённого baseline
и точной editor transaction. Каждый raw hash совпал с фактическим `disk_sha256`
успешного mutating SDK и отдельно проверенными bytes публикации.
CLI также сообщил 220 style/context diagnostics; они учтены отдельно и не
приняты как полная semantic/runtime-проверка 1С.

Foreign declarations не содержат UUID: для них сохранён прежний exact-root/path
replay, полная semantic identity ещё не доказана. Inline commands и legacy
family declarations предоставляют enclosing owner UUID и slot, но скрытый
child UUID не выдумывается. Эти границы не объявляются полной защитой всех
возможных подстановок.

Последующая [приёмка прерванного импорта](installed-native-recovery.md) проверила
`BackedUp` → `Complete`/`RollBack` для all4 в установленном клиенте.
Workspace-матрица, общий workflow EF-20 и полная EF-16/MVP приёмка остаются
открыты. Native build/dump этой частью не повторяются; физический ввод человеком
не заявляется.
