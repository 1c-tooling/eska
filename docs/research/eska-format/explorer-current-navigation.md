# Поиск и переходы по текущим метаданным Explorer

Commit Explorer `3e4c77e` продолжает [первую установленную приёмку](explorer-analyzer-foundation.md).
Дерево, поиск, свойства и поддержка используют тот же `eska-analyzer` и его
current metadata DTO. Второго индекса, собственного YAML parser или фонового
полного обхода при подключении нет.

Ключ объекта — source и UUID. После допустимого изменения имени текущий
parsed ID и label обновляются; неподтверждённые значения помечены pending
или unavailable. Порядок детей берётся из текущего объявления. Имена известных
типов и ролей модулей переведены на RU/EN; незнакомая роль остаётся видимой.

Поиск проверяет текущие имена и синонимы порциями до 200 owners, 2000 объектов
и 50 совпадений. Continuation сохраняет позицию и накопленную неполноту;
отказанные owners и остаток обхода показаны отдельно. Новый запрос, закрытие
picker или смена input epoch отзывают поздние результаты. Уже начавшийся
SDK metadata parse не объявляется прерываемым из UI.

Reveal использует только declared source → owner/module/query bindings,
проверенную ancestry и UUID. Он выделяет узел дерева без переоткрытия dirty
editor. Похожие имена или предполагаемый native path не заменяют объявление.

Поддержка запрашивается лениво для выводимых узлов через тот же host. Native
причина, freeze и неизвестный результат остаются разными значениями. Input
pending, restart и disconnect сразу отзывают старую policy. Ответ custom
metadata без действительного snapshot завершает сеанс. Открытие из дерева
пока всегда защищённое, даже при положительном policy result.

## Установленная проверка

Свежий VSIX `0.0.11` установлен в отдельный профиль VSCodium. Runtime содержит
только analyzer и необходимые launcher helpers; project CLI использован до
запуска для создания собственных CF/CFE/EPF/ERF. Бинарники заморожены между
перезапусками, пользовательский профиль не изменялся.

Для каждого типа прошли synonym search → native tree selection, exact reveal
dirty BSL без reopen, current root/inline rename с прежним source/UUID key,
pending/refused policy retraction, repair, dirty restart replay и отзыв pin.
В CFE дополнительно проверен freeze. Source bytes сохранены после проверок
и cleanup. Все 16 analyzer children успешного прогона завершены; завершены
также все 22 recorded children пяти попыток, собственные fixture/profile
каталоги удалены.

Проверка управляет настоящими QuickPick/TreeView через installed API. Строка
поиска и её callback задаются программно; тест включает `ignoreFocusOut` и
разрешает bounded retry только при скрытии UI в том же ready session.
Фактическое выделение native дерева проверяется отдельно. Ошибка snapshot
или backend restart не скрывается повтором. Это не физическая приёмка
клавиатуры, focus и всего интерфейса.

`npm run check`, полный Node suite **231/231 без skipped**, syntax обоих host
harness и diff check прошли. VSIX содержит 1310 entries, проверен отдельно;
payload совпал с исходниками. [Обезличенные результаты](explorer-current-navigation-results.json)
разделяют installed cases и границы.

EF-11 остаётся `IN_PROGRESS`. Полная прежняя Common/root/owner schema,
редактируемое открытие, property writes, `.sdbl` sync, CF/CFE composition,
vendor support installed matrix и большие проекты ещё требуют приёмки.
EF-21/EF-16 этим срезом не закрываются.
