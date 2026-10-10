# Переход к текущему объявлению BSL

`eska-analyzer` обслуживает стандартный `textDocument/definition` через общий
`AnalysisSession::module_definition`. При корректных frozen settings initialize
объявляет `definitionProvider=true`. Здесь описан первый BSL-компонент на
`0a700fd` со scope `ready-bsl-modules`. Текущий scope расширен в
[переходах из BSL к YAML](analyzer-metadata-definitions.md).
Отключённый ParseError не отключает навигацию; неверный settings file даёт явный
отказ языкового запроса. Metadata API остаётся доступным.

## Граница первого компонента

Ядро возвращает только BSL-цель из объявленного состава с ready текущим текстом.
Имя файла разрешается через существующий SourceRoot/FileSet, затем проверяется
по таблице модулей той же сессии. Дополнительного обратного индекса, обхода всех
файлов, копии target text или чтения диска для диапазона нет. UTF-8 границы и
SHA-256 проверяются по текущим байтам целевого модуля.

Локальное объявление может быть подтверждено при неполных соседних входах.
Переход к экспортной процедуре другого готового общего модуля использует также
его текущую каноническую YAML-модель. Invalid YAML очищает старый slot; замена
BSL меняет target range/hash, close восстанавливает фактический saved input.
Это подтверждение одного диапазона, не полная семантическая проверка проекта.

В закреплённом [goto_definition ядра](https://github.com/1c-tooling/bsl-analyzer/blob/9021ba07471eee8a2614bed2105a2c8699f60780/crates/ide/src/goto_definition.rs)
переходы к метаданным опираются на координаты XML. Эти пути не используются для
ESKA: сырой XML не загружается, диапазоны YAML не угадываются по имени.
Координаты YAML и первая связь BSL-символа с ними добавлены позднее; остальные
language operations и workspace search остаются отдельной работой.

## Запрос и результат

После initialize с ESKA protocol `0.1` и initialized клиент передаёт URI
объявленного локального BSL, UTF-16 position и optional `eska.snapshot`, как
в [completion](analyzer-completion.md). Snapshot при наличии проверяется до IO;
без него запрос использует текущую упорядоченную сессию. Lazy-чтение caller
может изменить ревизию. Pending соседние YAML/BSL автоматически не читаются.
workDoneToken и partialResultToken принимаются, потоковые части не отправляются.

Курсор в начале или сразу после идентификатора выбирает то же объявление,
что и курсор внутри имени. Общий выбор токена применяется к BSL- и YAML-целям:
соседние точка, скобка, пробел или конец файла не скрывают имя. Позиции внутри
комментариев и литералов не смещаются к чужим идентификаторам.

Подтверждённая цель возвращается как стандартный
[Location](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/language/definition.md):
URI и end-exclusive range, рассчитанный по target text. Location используется
также при client linkSupport=true; selectionRange без данных ядра не добавляется.
В расширении `eska` передаются snapshot, caller source/hash, target source/hash
и start/end byte, текущие resources и coverage. metadata_models_complete и
module_inputs_complete независимы; semantic_complete пока всегда false.
BOM, CRLF/LF/CR и non-BMP учитываются тем же borrowed converter, что другие
языковые запросы. Неверная позиция даёт position_invalid.

Если ядро не нашло цель при неполных моделях или module inputs, ответ —
`-32000`, navigation_incomplete, с текущим snapshot/hash/coverage/resources
в data.details. Пустой успешный ответ не скрывает неполноту. Если все текущие
поддерживаемые входы готовы и BSL-цель не найдена, результат `null`; это относится
только к объявленному scope. Неподдерживаемая цель даёт navigation_target_unsupported;
несогласованный диапазон ядра — navigation_inconsistent, `-32603`.

## Отмена и время

Initialize сообщает девять languageMethods, включая diagnostic, completion,
definition и [локальные prepareRename/rename](analyzer-local-rename.md);
десять cancellableMethods включают также preparePropertyEdit. Используются тот
же input thread, очередь на четыре сообщения, lifetime ID и RequestCancellation.
Клиентская отмена отвечает `-32800`, analysis_cancelled; deadline — `-32000`,
analysis_deadline_exceeded без retriggerRequest. `-32802` остаётся специальным
ответом diagnostic pull. `--timeout-seconds` по умолчанию 300, ноль отказывает
до lazy read; overflow имеет analysis_timeout_out_of_range.

Timer охватывает lazy read, kernel query, UTF-16 conversion и result, но не
initialize, очередь или wire write. При остановке готовые editor bytes/hash,
revision и resources сохраняются. Следующий запрос получает новый control.
Код без checkpoint не прерывается немедленно; верхней границы задержки нет.

## Проверки

Здесь сохранена проверка компонента на `0a700fd`. Последующий
[metadata locate](analyzer-metadata-locations.md) добавил отдельные координаты
объектов/свойств YAML; [metadata definitions](analyzer-metadata-definitions.md)
расширил стандартную навигацию верхнеуровневой целью текущего YAML,
а [member definitions](analyzer-member-definitions.md) — дочерними объектами.

37 unit/file, 39 analyzer process и 9 checker process tests прошли, все 19 tool
gates feature matrix/Rustdoc/release прошли. Root: 889 passed / 14 прежних ignored;
minimal 161/3, project-cli 177/3, build-cli 196/3, все 15 gates прошли.
Проверены local/cross-file targets, изменённый target BSL, invalid/close YAML,
current SHA/revision/resources, stale и неверные UTF-8/UTF-16 позиции, null только
при готовых входах, отмена с numeric/Unicode ID, нулевой/overflow timer и RU/EN.

C1/C2/C3 проверены в RU/EN: объявление существующего модуля, его сдвиг после
editor comment с non-BMP символом, stale refusal, close и incomplete refusal.
Полные machine traces совпали после нормализации opaque session identity.
Успешный локальный запрос не загружал YAML соседей.

| Corpus | Первый saved query EN/RU, с | Peak RSS EN/RU, KiB |
|---|---:|---:|
| C1 | 0,0222 / 0,0224 | 66296 / 66052 |
| C2 | 0,0241 / 0,0236 | 194548 / 194672 |
| C3 | 0,0263 / 0,0268 | 217808 / 217488 |

В C1 синтетический editor input 3600046 bytes отменён после наблюдаемого роста
RSS процесса до ответа. Ответ после cancel — 0,852/0,754 с. Timeout 1 с дал ответ
за 2,687/1,037 с: первый замер показывает задержку до следующего checkpoint.
Peak RSS deadline — 1131616/949060 KiB; source quota не ограничивает AST/memos.
Оба режима сохранили текущий ввод и позволили короткий saved query в том же
процессе с повторным ID. Числовой и Unicode ID нормализованы только в сравнении
этих двух намеренно различающихся traces. Это локальные замеры, не SLA.

PATH пустой, normal exit не ждал stdin EOF. Четыре проверенных файла каждого
corpus — manifest, configuration, выбранные YAML/BSL — сохранили bytes;
остальные файлы отдельно не хешировались. Временные settings C2/C3 удалены.
Платформа 1С и настоящий VS Code client в этом компоненте не запускались.

Release server — 48780736 bytes; runtime graph — 206 package names / 214 resolved
versions. Новых dependencies, CLI options или config keys нет.
[Машинный отчёт](analyzer-definitions-results.json) сохраняет gates и замеры.
Замеры относятся к первому BSL-компоненту. Вложенные BSL-цели YAML, все IDE edits,
полный memory budget и полнота анализа остаются в работе. EF-18 — `IN_PROGRESS`;
комплектная поставка и Explorer не переключены.
