# Связанная native пара CF/CFE

Для продолжения приёмки создана маленькая CF/CFE пара с настоящим заимствованием
справочника, его строкового реквизита и языка. Каждый original UUID принадлежит
явно выбранной базе. Исходники большой несовместимой пары не менялись.

## Платформа и импорт

На 1С 8.3.27.2325 собрана CF, затем CFE с явно переданным именно этим CF.
После независимой свежей выгрузки обоих артефактов все три ссылки сохранили
original UUID, kind, имя и родителя; несовпадений нет, `keepMappingIDs=true`.
В CFE у заимствованного `KeyЯ` явно сохранён String Type, совпадающий с CF.
Этот fixture не проверяет наследование отсутствующего Type или допустимость
свободного изменения проверяемого свойства расширения.

Первый `ibcmd config apply` завершился успешно, но Designer
`/CheckCanApplyConfigurationExtensions` обнаружил несовместимый режим интерфейса
тестового stencil. В собственной CFE исправлен только `InterfaceCompatibilityMode`
на фактическое значение CF (`Taxi`). UUID и требования проверки не ослаблялись.
После повторной сборки, загрузки и применения настоящий Designer подтвердил
применимость расширения с exit 0.

`/CheckModules -Server -ExternalConnection -ThickClientOrdinaryApplication`
прошёл для CF и CFE. В собственный модуль расширения введена заведомо отсутствующая
переменная: проверка дала exit 101 и три findings в нужном модуле. Выгрузка
текущего расширения подтвердила именно проверяемые BSL bytes. После восстановления
исходника повторены build/load/apply, проверка применимости, выгрузка и compile:
все прошли. Bytes/mode/mtime исходного BSL восстановлены.

CF и CFE импортированы в ESKA; три original UUID links сохранены. Явный
`bsl-analyzer.toml` выбирает одну базу и это расширение без зависимостей.
Все 89 исходных stencil/donor файлов сохранили bytes, mode, mtime, inode и hash.
Обе managed базы удалены через `eska clean`, собственных процессов 1С нет.

## Текущий release-analyzer

Проверен тот же frozen release, что прошёл 27 Rust gates и установленный Explorer
в [срезе хранилищ настроек](analyzer-settings-storages.md).
Framed Protocol 0.1 подтвердил два roots и selected extension. Открыт точный
сохранённый BSL с BOM/CRLF, без text changes и записи на диск.

Completion поля `KeyЯ` после `Catalogs.Sample.CreateItem()` вернул `String` за
0,040 s. `isIncomplete=false`; metadata inputs/models и module inputs complete.
Общий `semantic_complete` остаётся false.

Positional hover внутри имени того же поля вернул `null`. Координата проверена:
она находится внутри имени. Исходный строгий probe остановлен на этом результате
и сохранён. В ядре `resolve_name_to_definition` не разрешает обычный `FIELD_EXPR`,
хотя отдельный `symbol_for_typed_property` уже умеет возвращать тип свойства.
Нужный мост в positional symbol info пока отсутствует; прежний fallback EPF/ERF
эту CF/CFE ситуацию не покрывает.

Отдельный diagnostic-only процесс проверил сохранённый модуль того же расширения.
Он вернул полный отчёт за 0,042 s: metadata inputs/models, module inputs/syntax
и `semantic_profile_complete=true`, общий `semantic_complete=false`.
Четыре style/API findings сохранены: `CodeOutOfRegion`, `CommonModuleMissingAPI`
и два `MissingReturnedValueDescription`. Это не нулевой отчёт диагностики.
Повторных completion/hover запросов этот процесс не делал и найденный пробел
не устранил.

Оба analyzer процесса завершились штатно. Все 51+28 файлов ESKA проектов сохранили
bytes/mode/mtime. Процессы были короче первого интервала RSS sampling: нулевое
поле в raw receipt не означает отсутствие расхода памяти.

[Машинные результаты](native-matching-composition-results.json).

## Непроверенные условия

Эта пара подтверждает native association, применимость, compile с контрольной
ошибкой, импорт и текущий тип completion. Runtime функция и запрос не исполнялись;
установленный клиент на этой новой паре, физический ввод, Native Save, изменения
YAML/BSL/SDBL с обратной сборкой и повторный входящий импорт ещё нужны.
Positional hover поля CF/CFE остаётся конкретным незакрытым пунктом.
Совместимость с поставщиком и полный MVP не объявлены подтверждёнными.
