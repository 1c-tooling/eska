# Синтаксис `.sdbl` в установленном Explorer

Commit Explorer `538b17e` подключает [declared query syntax SDK](analyzer-query-syntax.md)
к тому же `AnalyzerConnection`, журналу документов и language provider, что
YAML/BSL. Он продолжает [приёмку навигации](explorer-current-navigation.md).
Установленный VSIX прошёл CF/CFE/EPF/ERF с frozen SDK checkpoint `2deec7f`.
Это syntax-only срез EF-11; контекстная EF-27 и полная приёмка MVP остаются открыты.

## Контракт клиента

Клиент передаёт `initializationOptions.eska.querySyntax=true` и проверяет точную
capability: languageId `sdbl`, identifier `eska-query-syntax`, scope `syntax_only`,
UTF-16, rules `[QueryParseError]`, metadata/consumer completeness false.
Редактор использует отдельный language id `eska-sdbl`; по wire передаётся `sdbl`.
Синхронизируются только query sources из проверенных owner declarations.
Грамматика не ждёт metadata paging или подтверждения текущего input.

Ответ diagnostics связан с текущими source, hash, editor version, origin и
snapshot; metadata/consumer/semantic coverage остаётся false. Каждая finding
должна иметь code `QueryParseError`: совпадения profile и coverage недостаточно,
чтобы принять BSL/semantic diagnostic. Неверный code отклоняется, прежние
diagnostics уже отозваны. Для query не запускаются BSL completion, hover, symbols
или metadata locate. Общие suggestions редактора этим не отключаются.

Query использует прежние revisions, quota, watcher, import guard, close и
restart/replay. Изменение input немедленно отзывает findings обеих URI-форм.
Epoch и token конкретного pull не дают позднему ответу или ошибке очистить
более новый результат; прежний ACK не подтверждает close/reopen с тем же
editor version. Отказ квоты сохраняет черновик и не подменяет его saved текстом.

Старый или незнакомый сервер сохраняет YAML/BSL и подсветку объявленного `.sdbl`,
но явно отказывает query analysis. Черновик остаётся в журнале для нового peer.
Если точная capability согласована, а query rule отключена policy, sync доступен,
синтаксическая диагностика недоступна. Реальная all4 проверка старого peer
подтвердила отказ без изменения revision, сохранение draft и replay в новый SDK.

## Исправление reveal

Установленная проверка обнаружила гонку: native tree reveal с полным parent chain
завершался успешно, затем lazy support repaint снова сворачивал ancestor до
прихода callbacks расширения, и выделение терялось. Клиент запоминает expansion
проверенной ancestry до native reveal. Epoch fences сохранены. Regression
проверяет реальный `getTreeItem` при repaint и отказ позднего reveal после
смены input epoch; установленная матрица проверяет фактическое выделение query.

## Проверки

Полный Node gate после production fix: **257/257, без skipped**. TypeScript check,
compile, проверка синтаксиса десяти test/harness файлов и diff check прошли.
Свежий VSIX версии `0.0.11` содержит 1311 entries, проверен на точный состав и
совпадение payload с текущими файлами. Финальное удаление одной пустой EOF-строки
из непоставляемой test fixture прошло syntax check; executable tokens и VSIX
не изменились. [Обезличенные результаты](explorer-query-syntax-results.json)
содержат digest проверенных собственных артефактов.

Unit tests покрывают точную negotiation, wrong-code guard, late result/error,
ACK, quota и same-version close/reopen. Настоящие Rust child processes через
production transport/provider проверены на all4 × RU/EN: current/stale inputs,
UTF-16 ranges с non-BMP символом, dirty watcher, удалённый файл при close,
restart, import guard и repair после отказа квоты. Editor API этого уровня
тестовый; отдельный legacy-peer прогон проверил all4 refuse → fresh replay.

Установленный VSIX проверен настоящим VSCodium в собственном профиле,
extensions directory и workspace. Runtime PATH содержит только analyzer и
необходимые launcher helpers; project CLI создаёт собственные fixtures до
запуска. Для каждого типа прошли protected/native query diagnostics, отзыв
старых protected findings, exact dirty reveal без переоткрытия, dirty watcher,
restart replay, import guard recovery и сохранение source bytes после cleanup.
Все 12 analyzer children успешного прогона завершены; завершены также все 23
recorded children восьми попыток, восемь наблюдавшихся fixture/profile roots
удалены. В отдельном legacy прогоне завершены ещё восемь children.

Native word-based completion вернул 9 suggestions для CF и по 12 для остальных
типов. Проверено отсутствие BSL requests при query completion/hover/symbol
commands, а не отсутствие любых suggestions. Harness вызывает настоящие
VSCode API/commands и возвращает нужный active editor после provider commands;
это не физическая проверка клавиатуры, мыши и focus.

После закрытия вкладок VSCodium удержал чистые скрытые `TextDocument` модели.
Установленный тест проверяет их чистоту, saved bytes и ACK модели, чей текст
совпадает с retained journal. Закрытие вкладок не выдано за `didClose`.
Удалённый query при настоящем protocol close проверен отдельно framed тестом.
Неудачные попытки и их traces сохранены локально отдельно от успешной матрицы.

Метаданные запроса, parameters/types, СКД и consumers, выполнение запроса на
платформе, CF/CFE semantic composition в установленном клиенте, editable tree
opening и property writes этим срезом не приняты. EF-11 остаётся `IN_PROGRESS`;
EF-21/EF-16 не закрываются, EF-27 сохраняет отдельную приёмку.
