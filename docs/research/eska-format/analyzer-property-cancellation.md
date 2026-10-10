# Отмена проверки базы при подготовке плана правки

`eska/metadata/preparePropertyEdit` использует общие RequestCancellation и Deadline
с диагностикой и completion. До открытия базы сохраняются проверки snapshot,
активного YAML, версии и SHA-256 caller-owned текста. Затем запускается timer:
`--timeout-seconds` по умолчанию 300, ноль отказывает до baseline IO.
Непредставимый timeout и невозможность создать timer имеют прежние структурированные
коды. Настройки BSL не являются условием подготовки свойства.

## Контроль запроса

Initialize сообщает три `cancellableMethods`: textDocument/diagnostic,
textDocument/completion, eska/metadata/preparePropertyEdit. Список languageMethods
остаётся из двух языковых методов. Включение editPlans по-прежнему требует
независимый accepted pin и клиентскую поддержку versioned documentChanges.

Input thread направляет [`$/cancelRequest`](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/specification.md#cancelRequest)
по текущему ID; worker единолично владеет сессией. Контроль и очередь общие для
всех трёх операций. Повторный активный ID любой из них отклоняется;
числовые/string IDs различаются. Завершённый или неизвестный ID не влияет на
будущее использование. Guard снимает регистрацию до начала wire write.

Клиентская отмена отвечает исходным ID, `-32800`, analysis_cancelled и snapshot
в data.details. Deadline плана отвечает `-32000`, analysis_deadline_exceeded
без retriggerRequest. Специальный `-32802` сохраняется только для diagnostic pull.
Первый сигнал определяет причину; поздний успех отбрасывается до wire write.
Отказ не возвращает пустой успешный план. После остановки текущие YAML
bytes/hash/version/revision и resource counters сохраняются.

## Проверка доверия

`AcceptedBase::open_cancellable` проверяет ту же полную базу, что прежний open:
pin manifest, обязательные blobs, composition, identity/ownership, native policy,
supplier payloads и freeze. Файлы не записываются, частичная база не возвращается.
Прежний open вызывает этот путь с неизменным false predicate; saved CLI/build
не получают таймер автоматически. У core нет зависимостей от IDE, Salsa или TTY.

Проверки остановки стоят между filesystem entries, порциями чтения 16 KiB,
разборами metadata и итоговой повторной проверкой inventory. Ограниченный text
reader сохраняет прежний 64 MiB hard limit; Interrupted read повторяется.
Core возвращает source_read_cancelled, транспорт переводит его в причину общего
контроля. Успешный план по-прежнему требует полной проверки базы при каждом запросе.

Это кооперативный лимит. Отдельный [Read](https://doc.rust-lang.org/std/io/trait.Read.html)
может блокироваться; YAML/JSON parse и CST preview не прерываются внутри одной
операции. Timer охватывает проверку базы, preview и построение UTF-16 edit/result,
но не initialize, очередь, preflight fingerprint, wire serialization или blocked
stdout. Backpressure может задержать ещё не прочитанную cancel notification.
Гарантия задержки ответа и адресный policy reader пока не реализованы.

## Проверки

35 unit/file, 35 analyzer process и 9 checker process tests прошли вместе с
19 gates tool feature matrix/Rustdoc/release. Root: 889 passed / 14 прежних
ignored; minimal 161/3, project-cli 177/3, build-cli 196/3, все 15 gates прошли.
Проверены отмена до IO и в середине/конце полного открытия, сохранность trust
и bytes, восстановление после остановки, corrupt pin, общий активный ID,
zero/overflow до отсутствующей базы и неизменность preflight-кодов в RU/EN.
Паритет locale и прежние диагностика/completion/checker сохранены.

В C1 отмена отправлена после подтверждённого роста Linux rchar не менее чем
на 8 MiB: это чтения процесса, включая page cache. Pin взят из отдельно
сохранённого результата прежнего import. Нормализованные полные machine traces
для RU/EN совпали; только opaque session identity исключена из сравнения.

| Locale | Чтение до cancel, bytes | Ответ после cancel, с | Повторный plan, с | Deadline 1 с: полный запрос, с |
|---|---:|---:|---:|---:|
| EN | 12955337 | 0,000428 | 3,799 | 1,012543 |
| RU | 15224075 | 0,000589 | 3,729 | 1,007117 |

После cancel в том же процессе получен обычный разрешённый plan с тем же ID.
После deadline прошла короткая диагностика BSL с повторным ID. Перед этими
запросами подтверждены неизменные editor snapshot/hash/resources. Peak RSS
cancel плюс повторный plan — 97872/98084 KiB; deadline — 70872/70820 KiB.
Это локальные измерения, не SLA и не общий memory budget.

PATH пустой, normal exit не ждал stdin EOF. Пять проверенных файлов — source
manifest/configuration, выбранные YAML/BSL и accepted manifest — сохранили bytes;
остальные файлы отдельно не хешировались. C2/C3, исполнение в платформе 1С и
настоящий VS Code client не проверялись в этом компоненте.

Release server — 48469360 bytes; runtime graph — 206 package names / 214 resolved
versions. Новых dependencies, CLI options или config keys нет.
[Машинный отчёт](analyzer-property-cancellation-results.json) содержит проверки и замеры.
Полная база всё ещё проверяется около четырёх секунд; адресная policy verification,
прочие IDE edits, memory budget и полнота анализа остаются в работе. EF-18 не завершена.
