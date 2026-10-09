# Адресный ввод канонических метаданных EF-18

Первый проверяемый компонент EF-18 — предложенное расширение `ide-db`.
[Patch](scripts/canonical-input.patch) применяется только к новой копии
bsl-analyzer `9021ba0` в playground. Рабочий checkout reference, протокол
действующего ESKA и опубликованные пакеты не меняются.

`CanonicalMdoInput` хранит готовый `MetadataObject` и source revision как
Salsa input. Структурный listing связывает main `FileId` с этим input через
отдельный map. Изменение значения input не меняет состав или идентичность
соседних объектов; XML-путь остаётся прежним для listings без canonical input.
Revision относится к точным bytes источника; семантически равная модель
может сохранить кеш вычислений, даже если изменились комментарии/координаты.

`None` в зарегистрированном canonical input означает pending/unavailable.
Resolver возвращает неполноту через отсутствие модели и не пробует XML.
Отсутствие slot и `None` в slot имеют разные значения. Адаптер ESKA должен
создать slots для объявленных объектов до запросов, проверять identity и
сообщать причину/полноту отдельно: одного `Option` недостаточно для отчёта CI.
Неверный YAML очищает текущее значение вместо возврата старого снимка.

[Consumer](scripts/canonical_probe.rs) открывает два ESKA-владельца и один
BSL-модуль в одном host. YAML передан в VFS как YAML, а не как XML или его
суррогат; BSL получает подготовленный объект через tracked input. Проверяются
появление draft-реквизита, сохранение структурного индекса, кеш соседнего
resolver, отсутствие повторной inference BSL после чужой правки, invalid/pending,
восстановление сохранённой модели и неизменность файлов.
Saved/overlay теперь проходят через общий `analysis_core::MetadataInputs`;
причина недоступности, input revision и полнота чтения сохраняются отдельно от
`Option` canonical input. [Контроллер](analysis-inputs.md) проверяет версии
запросов/документа, ограничивает один документ и сохраняет открытый черновик.

Проверка использует реальные Salsa events: до сравнения подтверждены две
выполненные object resolutions и две BSL inferences. После чужой правки
их счётчики не растут. `parse_mdo_query` не исполняется. Получение старой
строки из кеша или отсутствие наблюдаемого запроса не считается успехом.

```text
python docs/research/eska-format/scripts/probe_canonical.py --analyzer ../bsl-analyzer
```

[Runner](scripts/probe_canonical.py) проверяет commit/чистоту, экспортирует
read-only Git snapshot, проверяет patch, собирает отдельный consumer,
запускает его с пустым PATH и сохраняет результаты. Затем выполняет strict
Clippy всех targets `ide-db`/`ide-host-core`, полные unit-наборы этих библиотек.
Dependencies закреплены lockfile и тем же fork Salsa, что в EF-17.

Текущий consumer ограничен именами справочников и прямых реквизитов, типы
явно Unknown; `metadata_complete=false`. Для целого `Configuration` в
непроецируемых fallback-запросах задана пустая неизменяемая модель. Это
видимый предел эксперимента, не полная семантика и не замена общей проекции.

EF-18 остаётся незавершённой. Следующие обязательные части: остальные типы
канонического ввода и сопоставление свойств; полнота самого BSL-адаптера;
revision/cancellation/budget у общего IDE/batch host;
самостоятельные analyzer/check и их согласованная диагностика. Публикация
upstream patch и включение его в поставку пока не выполнялись.

Прогон прошёл build/strict Clippy consumer и всех targets изменённых библиотек,
174 unit `ide-db` и 30 unit `ide-host-core`, включая прежний XML-bootstrap.
Воспроизводимый consumer подтвердил обновление за 0,69 ms на маленьком примере;
это не замер больших конфигураций. [Машинный результат](canonical-input-results.json)
закрепляет patch, consumer и результаты. Для добавленного контроллера выполнены
полные fmt/check/strict Clippy/test/Rustdoc ESKA и набор source-only библиотеки.
Прошли 353 unit + 529 integration, 14 прежних проверок игнорируются; минимальная
библиотека — 159 unit и 3 игнорируемых. Среди них 12 новых проверок контроллера.
