# Семантический профиль пакетной проверки

`eska-check --profile semantic` выполняет настроенные native BSL rules через
ту же `AnalysisSession`, что языковые запросы SDK. Checker остаётся отдельным
одноразовым инструментом без IDE transport, Git, XML-экспорта и платформы 1С.
Настройки читаются общим loader из `bsl-analyzer.toml`; неподдержанные scopes
по-прежнему отклоняются. [Общий CLI/JSON-контракт](check-cli.md).

```bash
eska-check --profile semantic --format json
eska-check --profile semantic --project main --timeout-seconds 300
eska-check --profile semantic --max-retained-source-bytes 268435456
```

## Текущий ввод и полнота

Все YAML-владельцы и доступные объявленные BSL-тела загружаются до первого
diagnostic query. Соседний экспортный метод доступен при проверке вызывающего
модуля независимо от порядка путей. Недоступное тело остаётся unread: ядро
не получает старый текст или XML fallback и не угадывает ошибку отсутствующего
метода. Такие inputs делают весь результат неполным.

`syntax` сохраняет независимый streaming `ParseError`. В semantic конфигурация
rules не сужается до syntax; `ParseError` выполняется только при его включении.
Отключение всех rules даёт `profile_enabled=false`, неполную проверку и не
загружает тела. Exit 0/1/2/3 означают полный профиль без находок, находки при
полном покрытии, неполноту и ошибку запуска. Неполнота имеет приоритет.

Только semantic-профиль добавляет в schema 1 поле coverage `semantic_profile_complete`.
Оно истинно, если выбранные правила включены, все объявленные тела проверены,
YAML/BSL inputs и модели дескрипторов полны, остановки нет. У syntax поле
отсутствует. `syntax_complete=false` допустим при полном semantic-профиле,
если `ParseError` отключён. Settings fingerprint включает имя профиля;
прежний syntax fingerprint и JSON fields сохранены.

Это coverage выбранных native rules на поддержанных входах. Оно не подтверждает
все возможности языка, платформенную исполнимость или runtime. Текущий LSP pull
остаётся syntax; семантическая приёмка реального IDE-клиента не выполнена.
Экспериментальный Rust API изменён: `BatchReport.profile` теперь `BatchProfile`
вместо строки; `check_semantic` использует те же `BatchOptions` и результат.

## Ресурсы и представление

В semantic все ready тела остаются resident до конца анализа. Их совокупный
предел равен общей source-byte квоте YAML/BSL, по умолчанию 256 MiB.
Предел одного документа остаётся 64 MiB; отказ по квоте не маскируется как
успех. Общие deadline/cancellation действуют на reads, queries и coverage.
Временные объекты, результаты и memo в source-byte квоту не входят.

После каждых 32 результатов и в конце batch прежний deep trim сокращает окна
computed parse/source memos. Дополнительно удаляются saved reparse hints,
иначе resident semantic-тела могли бы удерживать все прежние syntax trees
вне Salsa LRU. Hints текущих редакторских документов, input bytes, origin,
source hashes и ревизии сохраняются. Новый кеш или дополнительная dependency
не добавлены. Это не общий heap/RSS budget.

JSON findings содержат прежние code/severity, byte ranges и Unicode scalar
positions. Human semantic findings показывают code/severity и позиции:
непрозрачный native prose не имеет единого структурированного RU/EN-контракта.
Summary, help, setup errors и причины YAML локализованы; SDK сохраняет native
message. Перевод текста правила по догадке не используется.

## Проверки

Регрессии проверяют межмодульный вызов с экспортным методом, отсутствующий
соседний BSL, совпадение diagnostics с текущей SDK-сессией, BOM/CRLF и координаты,
invalid metadata, binary, квоту, timeout, отключённые правила и разные profiles.
Process-тесты проверяют оба языка, exit codes, stdout/stderr и одинаковый JSON.
Native/SDK tests отдельно подтверждают сохранность editor hints и current inputs.

На настоящем C1 выбран `SelfAssign`: проверены все 1 975 объявленных модулей,
найдены 26 замечаний. Они совпали по code, severity и координатам с установленным
bsl-analyzer-app 0.3.0 на тех же 1 975 активных телах; его полный native обход
также увидел четыре необъявленных ресурса. YAML read complete, модели неполны:
результат сохранил exit 2 и `semantic_profile_complete=false`. `ParseError`
в этом выбранном наборе отключён, поэтому `syntax_complete=false`.

Оба языка дали одинаковый JSON с пустым PATH и без stderr. Текущий резерв:
46 327 997 bytes YAML и 68 881 821 bytes BSL, всего 115 209 818 bytes.
Наблюдаемый peak OS RSS — 475 912/475 572 KiB, время — 17,08/16,47 s RU/EN;
это единичные Linux-замеры, не предел памяти или гарантия производительности.
Все 7 829 исходных ESKA-файлов и 1 976 наблюдаемых native-файлов сохранены.

Собственный небольшой проект подтвердил SelfAssign и один действительно
отсутствующий метод с полным профильным coverage/exit 1; экспортный метод
не получил ложной находки. Native CLI подтвердил оба правила и ноль ParseError.
Workspace MCP недоступен, поэтому использован установленный CLI. Его platform
help сообщил degraded built-in facts с пустыми descriptions; эти проверки
не подтверждают полное platform help или выполнение в 1С.

Пройдены все 19 tool gates, 15 root gates и пять kernel gates, включая strict
Clippy и Rustdoc с warnings denied. Analysis all-features — 246 tests:
113 unit/file, 119 analyzer и 14 checker process; minimal — 91.
Kernel — 1 104 passed и четыре прежних ignored, включая 50 tests base-db.
Root — 895 passed/14 ignored; feature matrix сохранена. Новые три human keys
совпадают в обеих локалях. Normal graph обоих инструментов — прежние
206 имён/214 версий; manifest/lock и версии не менялись.

[Обезличенные результаты](semantic-check-results.json) фиксируют ограничения
отдельно от findings/coverage. EF-18 остаётся `IN_PROGRESS`: полная семантика,
общий memory budget, остальные IDE edits и Explorer ещё не завершены.
