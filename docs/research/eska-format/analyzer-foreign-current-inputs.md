# Текущие входы базы и зависимостей выбранной CFE

При выбранном составном CF/CFE Explorer раньше объявлял input membership только
собственного source root. Открытый буфер metadata зависимости сохранялся в
editor journal, но не отправлялся SDK: переименование `AField` оставляло анализ
запроса на сохранённой декларации. Real-process probe подтвердил этот случай.

SDK теперь объявляет `experimental.eska.inputDeclarations` версии 1 с методом
`eska/analysis/inputs`, scope `visible-foreign-inputs`, pageLimit 200 и
documentSync `existing-full-input-writer`. Параметры — точный snapshot, root,
offset и limit. Страница возвращает snapshot/root/offset/total и `inputs`:
каждый элемент содержит root, source, абсолютный file URI и languageId
`eska-source`, `bsl`, `sdbl` либо `xml`.

Читаются существующие root-local maps: owners, текстовые modules, queries и
зарегистрированные resources. Payload YAML/BSL/XML не открывается; revision и
quota не изменяются. Страницы не дают property-read/edit authority. Выбранный
root по-прежнему приходит через navigation owners; скрытая CFE и неизвестный
root получают `analysis_root_not_visible`. Метод использует прежние snapshot,
request cancellation/deadline и import retirement guards.

Explorer проверяет exact capability и URI/source/root/language membership до
первого dirty replay. Нативные буферы видимых базы и зависимостей проходят один
существующий writer, ACK и общий snapshot. Foreign ACK обязан совпасть по root,
source, origin и version; равный относительный путь selected root не подходит.
Pending metadata отзывает query findings; очередь запроса ждёт текущий ACK.
Invalid current base не подменяется saved metadata. Close, restart и import
guard сохраняют прежнюю current authority и правила её освобождения.

Foreign declarations расширяют синхронизацию входов. Дерево, property plans и
прямые language requests остаются в выбранном root. Hidden sibling может
оставаться открытым черновиком, но в выбранный context не отправляется.
Для composed semantic context новый Explorer требует exact input-declaration
capability; старый экспериментальный SDK получает явный analyzerIncompatible.
Standalone CF и прежний syntax route сохраняют свои границы.

Process tests SDK проверяют bounded pages RU/EN после удаления saved payload,
root visibility, snapshot, limit и deadline. Клиентские проверки с реальным SDK
подтверждают pre-connect foreign drafts, неизменный query buffer, repair/invalid,
close, hidden sibling, restart/import replay и сохранность файлов на RU/EN.
Полный Node gate — 334 passed, без failed/skipped. Отдельные transport tests
отвергают ACK от selected/missing root и другого source.

Это часть EF-18/EF-27. Matching native CF/CFE, полный анализ потребителей и весь
MVP не объявляются завершёнными.

Полный SDK gate прошёл fmt, check/Clippy/tests/Rustdoc с default и all-features,
debug/release builds: 213/511 tests без ignored, 11 gates. Все 1557 Rust/Cargo
files совпали до и после проверок. Main workspace этим изменением не затронут;
его предыдущие шесть gates (1508/1629 tests, по 24 ignored) сохраняются отдельно.

Свежий VSIX 0.0.11 установлен в отдельный VSCodium 1.109.51242 на Linux x86-64.
Configured CFE подтвердила текущие YAML базы и зависимости с одинаковым относительным
путём, repair/invalid, неизменную query version, hidden sibling и restart/import
replay. Файлы всех трёх foreign projects сохранены. CF, native CF и остальные
consumer cases также прошли; native standalone CFE явно отказала semantic context.
Все 14 analyzer children завершились. 54 compiled modules совпали с VSIX,
helpers исключены. VSIX SHA-256:
`05bc15ba5ddb99068002aa8c90e29657d1cf3cb078dc52ac49fae014aa42f8fe`.
Analyzer SHA-256:
`a796c619d852825bdcf49e9b9d49a30ef322ed574d358aa8258df79363faa76f`.
Это native editor API без физического ввода и Save.
