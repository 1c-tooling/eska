# Диагностика BSL через общий сервер

`eska-analyzer` поддерживает стандартный
[pull diagnostics LSP 3.17](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/language/pullDiagnostics.md)
через `textDocument/diagnostic`. Пока предоставляется профиль `syntax`
(`identifier=eska-syntax`), а не полная семантическая проверка. Ядро и selector
те же, что у `eska-check`; дублирования правил анализа нет.

## Настройки и согласование

`bsl-analyzer.toml` читается один раз при initialize из корня выбранного project
member. Общий loader и ограничение профиля переиспользуются checker/server.
Config fingerprint совпадает при одинаковых rules, locale не меняет fingerprint.
Изменение настроек требует restart; watcher/reload ещё не подключены.

Initialize объявляет `positionEncoding=utf-16`, `languageQueries=true`,
`languageMethods=[textDocument/diagnostic]`, состояние syntax в
`experimental.eska.analysis` и config fingerprint/path. Если settings корректны
и ParseError включён, также объявляется стандартный `diagnosticProvider` с
`identifier=eska-syntax`, `interFileDependencies=false`, `workspaceDiagnostics=false`.
Это свойство профиля syntax; полнота metadata/семантики из него не следует.

Неверный config и неподдержанный scope дают coded issue; дерево и document sync
остаются доступны. Disabled profile не объявляет diagnosticProvider и отвечает
`analysis_profile_disabled` на явный pull. Неизвестный identifier отклоняется
до чтения BSL. Source exclusions, extension/external/configuration dependencies,
diff/author/baseline scopes пока не поддержаны, как и в checker.

## Запрос и результат

Обычный запрос LSP использует последний ordered input. Опциональный ESKA context
позволяет потребовать конкретную ревизию; он проверяется до lazy-чтения.

```json
{
  "jsonrpc": "2.0",
  "id": 20,
  "method": "textDocument/diagnostic",
  "params": {
    "textDocument": {"uri": "file:///project/src/CommonModules/Tools/Ext/Module.bsl"},
    "identifier": "eska-syntax",
    "eska": {"snapshot": {"session_id": "<copied UUID>", "revision": "2"}}
  }
}
```

URI здесь синтетический. Действующий сервер принимает только declared local
module внутри своего канонического корня. Pending saved BSL читается один раз
с общей quota; текущий editor input не заменяется диском. Missing/binary/refused
input даёт error `module_unavailable` с текущим snapshot/state/resources.
Повторный pull не перечитывает failed input и не возвращает успешно пустой список.

Ответ — стандартный full document report: `kind=full`, opaque `resultId`, `items`.
`previousResultId` и progress tokens принимаются, но incremental/unchanged reports
и progress events пока не используются; возвращается полный результат.
В каждом finding — UTF-16 range, LSP severity, code, source, human message и
canonical `start_byte`/`end_byte`/original severity в `data`.

Сообщения используют RU/EN ядра. Закреплённый ParseError handler игнорирует locale;
общая сессия выбирает английское представление из структурированных ошибок
того же cached parse. Русский текст не переводится подстановкой фраз.
Нетипизированные custom reasons дают общее `Syntax error` в EN; подробная
английская причина требует типизации в parser. Русский текст сохранён.
Codes, severity, координаты, fingerprint,
coverage и JSON shape не зависят от process locale; при сравнении исключаются
human messages и случайные connection/report identities. Это отличается от
машинного JSON checker, который вообще не сериализует human message.

Дополнительный `eska` в report содержит snapshot, source/hash, profile/config
fingerprint, resources и coverage. `module_syntax_complete=true` означает
выполненную проверку этого текста, включая найденные syntax errors.
`metadata_inputs_complete`, `metadata_models_complete`, `module_inputs_complete`
показывают текущее покрытие проекта отдельно; `semantic_complete=false`.
Пустые items при готовом текущем тексте не считаются доказательством полной
семантической корректности конфигурации.

Byte endpoints проверяются перед конвертацией; они не округляются к границе
другого codepoint. Все требуемые позиции рассчитываются одним проходом по
текущим bytes, с UTF-16 units, CRLF и end-exclusive range. BOM учитывается как
часть переданного текста: overlay отражает строку клиента, saved — точные
UTF-8 bytes файла. Core даёт borrowed callback над текущим текстом с revision/hash,
не хранит LSP-модель и не создаёт дополнительную копию всего модуля.

## Выполненные проверки

26 unit/file, 21 analyzer process и 9 checker process tests прошли. Проверены
saved/editor/close, invalid/recovery, disabled/invalid/unsupported settings,
restart/config snapshot, stale context до чтения, стандартный запрос без ESKA
context, UTF-16 Unicode/BOM/CRLF/EOF и совпадение с shared batch по codes,
важности, byte ranges и сообщениям. fmt/check/strict Clippy/test/Rustdoc
прошли для всех tool features. Отдельный kernel workspace: 1054 passed /
4 прежних ignored, включая восемь тестов formatter ошибок парсера; все gates прошли. Root: 887 passed / 14 прежних ignored;
minimal — 161/3, project-cli — 177/3, build-cli — 196/3; обязательные gates прошли.

C1/C2/C3 проверены в обеих локалях: изменение YAML, syntax pull редакторского
BSL, две ParseError в повреждённом черновике, save/close и восстановление
нулевой диагностики saved текста. Финальная общая ревизия — `6`.
UTF-16 ranges сверены независимым расчётом по source bytes.

| Corpus | Peak RSS EN/RU, KiB | Findings draft / invalid / closed |
|---|---:|---:|
| C1 | 65464 / 65960 | 0 / 2 / 0 |
| C2 | 193288 / 193568 | 0 / 2 / 0 |
| C3 | 216472 / 216880 | 0 / 2 / 0 |

В этих transport-сценариях проверены четыре исходных файла каждого corpus:
manifest, configuration, выбранный YAML и BSL; байты сохранены. Остальные
файлы не хешировались. Полный checker C1 проверил все 1975 активных модулей,
нашёл 0 syntax findings и сохранил exit 2 из-за неполной metadata model.
У выбранного saved BSL совпали source hash, config fingerprint и findings server/checker.

`PATH` пустой; временные настройки собственных C2/C3 удалены. Graph по-прежнему
206 package names / 214 resolved versions; `project-model` стал прямой optional
зависимостью analyzer feature, но уже был в graph. Release server — 46606488 bytes.
Git, 1C, HTTP-клиент, debugger и LSP runtime не подключены.

[Машинный отчёт](analyzer-diagnostics-results.json) сохраняет gates и замеры.
Сборка 1С не запускалась: проверены язык/transport, не платформенная приёмка.
Gates и замеры выше относятся к `fc26d82`. [Отмена активной диагностики](analyzer-cancellation.md)
добавлена следующим компонентом. [Server timeout](analyzer-deadline.md) уже подключён;
текущий размер сервера и проверки — по этой ссылке.
Полный heap/RSS budget, completion, семантика и edit plans не завершены.
Source-byte quota не ограничивает memos или общую память процесса.
EF-18 остаётся `IN_PROGRESS`.
