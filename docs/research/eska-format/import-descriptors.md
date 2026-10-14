# Импорт независимых правок одного descriptor

Это продолжение EF-25 поверх [native/project plan](native-project-import.md).
Подтверждённый предок, incoming и working сравниваются прежним source plan;
support/freeze по-прежнему определяет отдельно закреплённая база. Новый pin,
повторное принятие incoming и новый CLI не создаются.

Если обе стороны изменили owner YAML, plan сравнивает значения его mapping.
Независимые поля объединяются: например, local Synonym и incoming Comment
одного корня. Коллекции пока атомарны: incoming может добавить/удалить/переименовать
вложенный элемент, если working не менял эту коллекцию. Local Comment владельца
при этом сохраняется. Изменение удаляемого элемента даёт `both_modified`
на координате коллекции. BSL, SDBL, binary и остальные payloads остаются файлами
для трёхстороннего сравнения, без нового текстового merge engine.

Working YAML остаётся основой. По CST копируются только выбранные incoming
value subtrees; соседние keys, annotations и bytes не сериализуются заново.
Переход `[]` в block collection и empty-state mapping в scalar сохраняет
working CRLF. После вставки повторно проверяются полный semantic value и точная
кратность комментариев обеих сторон. Необрабатываемая concrete syntax, потеря
комментария или incoming annotation вне выбранных subtrees оставляют конфликт;
serializer fallback отсутствует. Formatting incoming вне выбранных values
не переносится. Совместные правки двух членов одной коллекции пока конфликтуют.

`SourceImportConflict.coordinate` — optional JSON field со schema keys; пустая
координата не сериализуется, прежние file-only conflicts сохраняют свою форму.
Merged text получает собственный exact digest в selected/output inventory.
Publication и recovery используют те же rechecks, candidate и guard: исходный
ancestor/incoming не заменяется merged data с локальными правками.

Результат проходит прежний protected selector, reference validation и native
codec. Нет разрешения на новые top-level files или изменение reverse map.
Поддержаны существующие inline creation/deletion/rename profiles. Для нового
реквизита справочника дополнительно принят полный default tail, который
1С 8.3.27.2325 добавила после native build/dump: Name/Synonym/Comment/Type и
точный набор 25 default properties. Неполный, переставленный, неизвестный или
изменённый tail отклоняется. Это не общий профиль произвольных новых native
реквизитов. Fresh UUID получает право создания от принятого родителя; не
превращается в accepted object, freeze/native restriction родителя остаются.

## Проверки

Девять новых unit-сценариев проверяют inline add/delete/rename с local owner
value, пересечение значений, delete-versus-edit, freeze, комментарии, stale input,
переход пустой коллекции, scalar/flow syntax и recovery без исходных stores.
Default profile дополнительно проверен на partial/changed/reordered/extra tails
и frozen parent. Повтор source import из сохранённого raw ancestor не создаёт
изменений. Full native repeat с новой service выгрузкой остаётся отдельной задачей.

Все 22 Rust gates прошли: root 795 unit/600 integration; all-features также
21 build/50 project process tests; SDK 185/158/19, adapter и minimal build
checks. Root пропускает 24 opt-in случая; они не считаются подтверждёнными здесь.

На четырёх native inputs проверены local Synonym/root annotation и incoming
Comment в одном owner, плюс independent local BSL. В CF также добавлен boolean
реквизит справочника при local Comment этого же владельца. CF/CFE/EPF/ERF прошли
unpack/apply/build/dump на 1С 8.3.27.2325, settings и прежний accepted pin
сохранены. 13/107/8/4 native files, 3/25/3/1 BSL modules: BSL/opaque bytes совпали,
metadata XML values/structure совпали с материализованным control.

`ConfigDumpInfo.xml` остаётся прежним pinned bookkeeping в исходниках.
Свежая выгрузка CF помимо service configVersion добавила ровно одну Metadata
запись нового реквизита. Сравнение отдельно подтвердило её UUID/имя и отсутствие
прочих изменений; полный CDF byte/semantic no-op для этого случая не заявляется.
В остальных трёх профилях исключены только service configVersion.
[Обезличенные результаты](import-descriptors-results.json).

Workspace MCP BSL Analyzer отсутствует; установленный CLI 0.3.2 проверил
затронутые native modules. Подробный результат указан в JSON. Это синтаксическая
проверка owned copies, а не исполнение новых метаданных в рабочем клиенте.

Новая карта top-level add/delete/UUID-preserving rename, заимствованные объекты,
native CLI lifecycle и согласованное обновление IDE ещё нужны до закрытия EF-25.
Windows/macOS, боевые конфигурации, memory budget и клиентский runtime этой
частью не приняты. Версии source schemas и crate не изменены.
