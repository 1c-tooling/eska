# Делегирование `eska check`

Оболочка запускает отдельный `eska-check` для выбранного ESKA-проекта/member.
Диагностические правила, discovery, настройки и покрытие остаются в
[общем checker](check-cli.md). Designer XML этот маршрут не поддерживает.

```sh
eska check --profile syntax --format json
eska --project-dir workspace check --project main --profile semantic
eska check --profile syntax --config bsl-analyzer.toml --timeout-seconds 30
```

Профиль обязателен. Относительные `--project-dir` и `--config` сохраняют смысл
самостоятельного инструмента; `--config` разрешается выбранным checker относительно
project root. Неуказанные timeout/document/source limits остаются defaults checker.
`--lang` передаётся в канонической форме `ru-RU`/`en-US`.

Сначала выбирается sibling executable рядом с оболочкой, затем первый regular
candidate в PATH. Выбранный бинарник больше не заменяется после неудачи.
Перед анализом выполняется `--capabilities --format json`: максимум stdout
65 536 bytes и пять секунд. Проверяются tool/envelope, protocol/report schema,
точная `source_core_version` оболочки, выбранный profile и source format.
SDK package version сохраняется отдельно: `0.0.0` не подменяется версией core.
Probe не открывает проект и не запускает платформу.

После admission stdin/stdout/stderr передаются напрямую. JSON результата
`eska-check` не пересериализуется; сохраняются numeric exit и Unix signal status.
Неудача shell discovery/probe/start даёт exit 3 и stable `check_tool_*` code;
JSON имеет tool `eska`, command `check`. Help и ошибки parsing самой оболочки
сохраняют прежние exit 0/2. Поддержка профиля не означает semantic completeness.

Пять реальных process tests проверяют RU/EN, arguments с пробелами/Unicode,
stdin с бинарными bytes, raw streams/status, sibling/PATH, отказ несовместимости,
bounded probe и help/required profile. Unit admission проверяет точные typed
fields. Две checker process groups подтверждают standalone capabilities,
отсутствие project reads и отказ смешанных analysis arguments.

На восьми actual CLI случаях (CF/CFE/EPF/ERF × RU/EN) direct и shell checker
совпали побайтно по stdout/stderr и exit code. CF вернул 0, остальные типы — 2
с явной неполнотой контекста; syntax_complete был true. Все source/control/
accepted/history bytes и mtimes сохранились, runtime PATH был пустым.
Платформа и full semantic acceptance здесь не проверялись.
[Обезличенная матрица](shell-check-delegation-results.json).
