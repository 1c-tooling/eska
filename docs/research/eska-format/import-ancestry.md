# Подтверждённый предок и библиотечный план импорта

Вторая часть EF-25 добавляет отдельные `ImportAncestor` и `ImportAncestorRef`
в project core. Предок хранит исходники последней синхронизации; `AcceptedBase`
по-прежнему определяет поддержку и дополнительный freeze. Тип ссылки предка
нельзя передать вместо `AcceptedRef` в защищённую подготовку или сборку.

## Снимок исходников

`capture_import_ancestor(source, destination)` проверяет полную обратимую
проекцию и записывает неизменяемый снимок в новый каталог. Это явный выбор
входных данных, а не автоматическое признание текущей рабочей копии предком.
Caller должен подтвердить, что выбранный снимок является общим предком.

`ancestor.json` schema 1 содержит `schema_version`, `source_revision`, `files`
и `directories`. В `blobs/<sha256>` находятся точные bytes исходников;
одинаковые blobs записываются один раз. В marker нет supplier policy или freeze:
нативные правила могут быть файлами внутри снимка, но не предоставляют права.
Marker публикуется последним. Его SHA-256 подтверждается отдельным
`ImportAncestorRef`; `open` не выбирает сумму из прочитанного каталога.

Проверка требует точный состав blobs и каталогов, корректные пути и hierarchy,
согласованный source revision и полную валидную проекцию. Изменённые, отсутствующие
и лишние файлы дают отказ. `verify()` повторно проверяет открытый снимок;
`open_cancellable` не возвращает частично проверенный предок. `restore`
создаёт только новый или полностью совпадающий каталог исходников, сохраняя
binary bytes, BOM/CRLF и пустые каталоги. Он не создаёт принятую базу.
При потере питания возможен незавершённый новый store; повтор не перезаписывает
чужой или неполный каталог. Project journal остаётся следующей частью EF-25.

## План и текущая граница

`prepare_source_import(source, ancestor, incoming, approved)` использует четыре
входа: текущие исходники, подтверждённый предок, входящий снимок и отдельно
одобренную базу. Все четыре revisions/storage проверяются перед публикацией
и после неё. CF/CFE с одинаковым UUID различаются по полному native type;
другой root UUID, профиль или схема дают отказ.

Сопоставление по UUID и native slot общее с EF-15. Сравнение пока выполняется
на уровне файла: изменился только incoming — выбирается incoming; только working —
сохраняется working; одинаковые правки согласуются; разные правки одного файла
дают конфликт. Это консервативная граница, не семантическое YAML/BSL слияние.
Preview содержит exact entries предка/incoming/working/selected и причины
конфликтов. Удаление против правки и binary conflict не выбираются молча;
лишние рабочие файлы и пустые каталоги не отбрасываются.

Изменение `ParentConfigurations.bin` или supplier CF требует EF-15.
Исходящий вариант проходит тот же protected selector и структурные проверки,
что обычный экспорт; forbidden edits содержат точные `Exclusion` с исходным pin.
`publish_source` записывает проверенную отдельную копию и не меняет working,
ancestor, accepted storage, CI pin или последний build.

Первый профиль сохраняет approved reverse map. Изменение UUID, добавление,
удаление или rename, требующие другой карты, дают `mapping_unsupported` и точные
сравниваемые entries. Это ещё не завершённая EF-25: нужно подключить проверенную
новую карту, ancestor lifecycle, импорт в существующий проект/member с recovery
и согласованное обновление IDE. Пользовательская команда ESKA reimport пока
не публикуется; библиотечный candidate не является её заменой.

## Проверки

Одиннадцать unit-сценариев проверяют snapshot/restore/repeat, независимость
от рабочих правок и freeze, неправильный pin, corruption/extra/missing,
отмену, overlap и foreign output. План сохраняет независимые правки, отказывает
при обеих правках файла, удалении рабочей стороны против incoming edit,
замене UUID, freeze и изменении policy bytes; no-op и stale input проверены.
CF/CFE с одинаковым UUID и другой UUID дают разные явные отказы.

Все 22 Rust gates прошли. Root: 768 unit, 600 integration; all-features
дополнительно 21 build и 49 project process tests. SDK: 185/158/19;
minimal build CLI: 603 unit и 21 process test. Обычный root прогон пропускает
24 opt-in случая, они не считаются подтверждёнными этой частью.

Отдельный source-only consumer использует принятые CF/CFE/EPF/ERF выгрузки
1С 8.3.27.2325, query layout 5/6 и собственные каталоги playground. Для каждого
сохранена локальная правка BSL, подтверждены повторная candidate publication,
strict protected prepare и неизменный pin/снимки. Все четыре candidates прошли native build/dump на 1С 8.3.27.2325:
13/107/8/4 файлов, 3/25/3/1 BSL modules сохранили точные bytes. Descriptor XML
различается форматированием; структура, attrs и непустые text values совпали
при исключении только service `configVersion`. Модули с локальной правкой
сохранились после повторной выгрузки. Это independent source-only consumer,
а не ESKA reimport command или application к рабочей ИБ.
[Обезличенные результаты](import-ancestry-results.json).

Workspace MCP анализатора в сеансе недоступен; установленный CLI 0.3.2 проверил
четыре изменённых native module copies: completed/failed_files=0, SyntaxError
не найден. Он также вернул style/context diagnostics, в том числе
missing BSP symbols для внешнего объекта; отсутствие полной семантики не
выдаётся за production acceptance. Initial fixture через старый readable
creation API отказала для EPF; применён существующий явный query import API,
не изменяющий старые defaults. Общий new/init остаётся EF-20.
