# Комментарий в текущем YAML-буфере Explorer

Следующая часть EF-11 добавляет **Изменить комментарий…** для существующего
`Comment` конфигурации и настроенного расширения. Действие использует прежний
`eska/metadata/preparePropertyEdit` и уже открытый обычный YAML-редактор.
Второго редактируемого YAML в панели свойств нет. Открытие из дерева и панель
свойств пока сохраняют readonly поведение.

## Проверка и применение

Клиент допускает только точный optional contract: editPlans и versioned
propertyEditPlans с open-document scope; initialize сообщает поддержку
workspaceEdit.documentChanges. Независимо переданный accepted-base pin,
текущие свойства/UUID и разрешение всего физического owner проверяются заново.
Неизвестный pin/policy, запрещённый inline объект, защищённая, закрытая,
скрытая или дублирующая native URI вкладка не получают правку.

Запрос использует текущие text/hash, source, UUID и wire document version.
Native editor version проверяется отдельно. Ответ допускает один TextDocumentEdit
точного URI/version с нулём правок для no-op или одной plain UTF-16 правкой.
Resource operations, другие документы, неверные ranges и разрезанный surrogate
pair отвергаются. После ввода и перед применением повторно проверяются current
text/version, snapshot, tree generation, connection epoch и pin.

Пользователь видит значение в обычном input и отдельном подтверждении.
`TextEditor.edit` применяет одну native транзакцию с undo/redo. Обычный observer
остаётся единственным didChange writer; действие ждёт его ACK. Отказ не
перезаписывает черновик старым текстом. Restart/import retirement использует
прежний journal/replay.

Правка остаётся dirty. Обычный Save не проходит через SourceEditPlan;
перехват Save, guarded publication и прямой disk write этой частью не добавлены.
Она не принимает полное editable opening, произвольные property edits или MVP.

## Проверки

Explorer commit `84ea240` сохраняет version 0.0.11. Полный Node suite — 284/284,
без skipped; TypeScript check/compile, RU/EN key parity, harness syntax и diff
прошли. Direct consumer проверен с native version 41 и wire 7; реальный framed
producer — с native 41/wire 1. Покрыты CRLF/Unicode, stale dialogs/responses,
неизвестные права, frozen/locked/shared source, restart/import и rejected ACK.

Свежий VSIX с 1313 entries проверен в установленном VSCodium 1.109.5 на Linux
с immutable analyzer из принятой composition-сборки. Exact bytes пары и всех
packaged production files после приёмки сохранились. Fixtures — собственные
синтетические native declarations, импортированные реальным project tool;
configured CFE использует явный base, это не платформенная проверка совместимости.

Оба installed случая прошли actual public command, native input/confirmation,
текущий typed Comment, dirty-only edit, undo/redo, restart и import/replay.
На installed пути native/wire версии обе равнялись 3; их расхождение доказано
отдельными direct/framed тестами. Четыре authoring files каждого fixture
сохранены побайтно, Save не вызывался. Это API/CDP-driven приёмка, не физические
клавиатура/мышь.

VS Code test host запрещал modal dialog до подтверждения. Production UX не
менялся ради этого ограничения: итоговый прогон использовал изолированный
обычный host и собственное helper extension без extensionTestsPath. Helper
не входит в VSIX, читает уже работающий Explorer и не создаёт второй backend.
Неуспешные traces сохранены отдельно. Все 11 analyzer children успешного и
предыдущих прогонов завершены; шесть собственных profile/fixture roots удалены.
[Обезличенные результаты](explorer-comment-buffer-results.json).
