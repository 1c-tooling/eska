# Форматирование ESKA

## Статус контракта

T120 реализует политику в общем Rust core: профиль, схема, resolver, migration
preview/apply и протокол текста. Самостоятельный `eska-fmt`, алгоритмы BSL/SDBL
и интерфейс IDE следуют в T121–T144; команды ниже пока не объявляются доступными.
Анализатор не требуется для разрешения настроек или форматирования.

Первичный конфиг — `eska.toml`, секция `[format]`.
[JSON Schema версии 1](../schemas/format-v1.schema.json) описывает эту секцию,
а не целый манифест. Численные ограничения и область применения проверяет также
строгий загрузчик проекта/workspace. Смена версии профиля требует явного выбора;
новая версия приложения сама не меняет defaults профиля `1c` версии 1.

## Конфигурация

```toml
[format]
profile = "team"
profile_version = 1
exclude = ["**/generated/**"]

[format.profiles.team]
based_on = "1c"

[format.profiles.team.common]
column_limit = 100

[format.bsl]
indent_style = "space"
indent_width = 4

[[format.overrides]]
include = ["src/CommonModules/Legacy/**"]

[format.overrides.bsl]
column_limit = 0
```

Без секции используется `1c` версии 1. Существующие явные параметры
`tools.bsl-analyzer.formatting` учитываются для BSL. При несовместимости со
значением новой секции операция отклоняется до записи. `use_tabs=true` и
`indent_size=1` означают один таб шириной 4 колонки; другой размер в режиме
табуляции неоднозначен и требует явной миграции. В режиме пробелов сохраняется
число пробелов. `indent_size` не становится шириной табуляции автоматически.

Порядок: разрешённый профиль → workspace common/язык → member common/язык →
overrides workspace, затем member → параметры операции. В одном списке overrides
последнее совпадение побеждает. `include` обязательно, `exclude` внутри правила
отменяет только это правило. Пути относительно объявившего правила `eska.toml`;
поддерживаются `*`, `?`, `**` и `/`. `**/` совпадает также с нулём каталогов.
Абсолютные пути, `..`, обратные слеши и классы `[]` в glob не принимаются.

Таблицы наследуют отдельные ключи; скаляры и списки заменяются целиком.
`exclude=[]` очищает наследованные исключения, `column_limit=0` отключает
ограничение, `false` сохраняет свой смысл. Исключения не снимают защиту исходников.
Пользовательские профили объявляются в `format.profiles.<name>` и наследуют
`based_on`; встроенный `1c` переопределить нельзя. Неизвестные профили и циклы
отклоняются. Параметры отступов редактора не переопределяют стиль проекта.

## Каталог опций версии 1

Общие параметры допустимы в `common`, `bsl`, `sdbl` и соответствующих таблицах
профилей/overrides. `keyword_case` профиля SDBL по умолчанию `upper`, BSL —
`canonical`. Типы, диапазоны, enum и остальные defaults входят в JSON Schema.

| Ключ | Default | Смысл / English |
| --- | --- | --- |
| `indent_style` | `tab` | `tab` или `space`; indentation character |
| `indent_width` | `4` | Колонок на уровень; columns per block |
| `tab_width` | `4` | Визуальная ширина таба; tab stop width |
| `continuation_indent_width` | `4` | Добавочный отступ продолжения; continuation columns |
| `column_limit` | `120` | Ориентир длины, `0` без лимита; soft column limit |
| `keyword_case` | По языку | `canonical`, `upper`, `lower`, `preserve`; token spelling |
| `space_after_comma` | `true` | Пробел после запятой; comma spacing |
| `space_around_operators` | `true` | Пробелы вокруг бинарных операторов; binary spacing |
| `space_inside_parentheses` | `false` | Внутренние пробелы скобок; inner parentheses spacing |
| `space_before_call_parenthesis` | `false` | Пробел перед скобкой вызова; call parentheses spacing |
| `wrap_arguments` | `fit` | `fit`, `one_per_line`, `preserve`; call argument layout |
| `wrap_binary` | `fit` | `fit`, `one_per_line`, `preserve`; binary expression layout |
| `break_before_binary_operators` | `true` | Оператор начинает продолжение; leading operator |
| `max_empty_lines` | `1` | Максимум пустых строк; consecutive empty lines |
| `trim_trailing_whitespace` | `true` | Удаление хвостовых пробелов вне литералов/комментариев; trailing trivia |
| `insert_final_newline` | `true` | Финальный перевод строки; final newline |
| `comments` | `indent_only` | `indent_only` или `preserve`; comment indentation |
| `align_assignments` | `false` | Только BSL: выравнивание соседних присваиваний; assignment columns |
| `blank_lines_between_methods` | `1` | Только BSL: пустые строки между методами; method separation |
| `select_items` | `one_per_line` | Только SDBL: `fit`, `one_per_line`, `preserve`; SELECT item layout |
| `clause_newlines` | `true` | Только SDBL: секции с новой строки; clause separation |
| `align_aliases` | `false` | Только SDBL: выравнивание КАК/AS; alias columns |

Например, ширина 2 и `space` дают два пробела вместо таба; `space_after_comma`
переключает `Метод(А,Б)` ↔ `Метод(А, Б)`; `space_around_operators` — `А=Б+В` ↔
`А = Б + В`; `keyword_case` — `если` ↔ `Если`/`ЕСЛИ`; `wrap_arguments` —
`Метод(А, Б)` ↔ вызов с каждым аргументом на своей строке; `select_items` —
`ВЫБРАТЬ А, Б` ↔ список полей по строкам; `align_aliases`/`align_assignments`
добавляют пробелы до общей колонки. Полные golden-примеры алгоритмов относятся
к T121/T122: наличие опции в политике пока не объявляет её алгоритм готовым.

## Миграция и машинный контракт

Миграция выполняется явно: preview содержит точный исходный текст и результат.
Apply повторно проверяет план, текущие байты, тип файла и права. До замены создаёт
исключительно новый `.eska-fmt-backup-<sha256>.toml` рядом с манифестом; совпавший
backup допускается только при точном совпадении байтов. Чужой backup не заменяется.
Повтор apply к уже применённому плану возвращает тот же backup. BOM/CRLF, прочие
настройки и комментарии вне переносимой секции сохраняются. Подменённый план,
изменившийся манифест или защищённый файл не применяются. Обычное форматирование
миграцию манифеста не выполняет.

Протокол форматтера имеет `schema_version=1`: `language` (`bsl`/`sdbl`), полный
`options`, снимок `text`, nullable `range`. Координаты — полуоткрытые UTF-8 byte
offsets, границы проверяются до разбора. Лимит снимка — 4 MiB. Ответ содержит
`source_sha256`, список `edits` и фактический `formatted_range`. Адаптер IDE
преобразует координаты в нативные и проверяет версию своего документа; форматтер
не получает команды сохранить владельца. Полная эффективная политика содержит
происхождение каждой настройки, версию профиля, язык и признак исключения.
