# Текущие критерии отбора

`FilterCriterion` имеет отдельные canonical модель и listing. Общий адаптер
читает UUID, имя, `Type`, `Content` и ownership из текущего `ParsedObject`.
Критерий не подменяется MDO или StructureOnly; XML sidecar и второй parser
не используются.

Платформа задаёт тип поиска и состав объектов отдельно: результат содержит
объекты, реквизиты которых удовлетворяют критерию.
[Описание 1С](https://v8.1c.ru/platforma/kriteriy-otbora/) и
[руководство разработчика](https://1c-dn.com/library/tutorials/practical_developer_guide_understanding_filter_criteria/)
подтверждают это разделение. Reference MCP закреплённой справки подтвердил
`FilterCriteriaManager`, сгенерированный `FilterCriterionManager.<имя>` и
`Find(<значение поиска>)`, возвращающий массив.

## Native потребители

Настоящий глобальный контейнер `КритерииОтбора` / `FilterCriteria` получает
только имена из видимого текущего состава. Поле возвращает platform object
с настоящим generated type `FilterCriterionManager.<имя>`; новый фиктивный
`MdoType` не добавляется.

Методы сохраняют native ID, arity, execution environments и call candidates.
Общий текущий `TypeDescription` задаёт тип аргумента `Найти` / `Find`.
Массив результата содержит объединение ссылок на владельцев полей из `Content`;
индексирование сохраняет этот тип. Полные пути полей остаются в canonical модели.
Результат не превращается в таблицу или в значение поискового реквизита.

Закреплённая справка содержит generic placeholder в именах методов и ошибочный
контекстно-независимый тип аргумента поиска. Callable spelling берётся из той же
native документации. Inference уточняет типы native candidates из текущего
критерия. Hover и signature help используют эту привязку вызова и native
рендеринг, сохраняя ID и документацию; placeholder не выдаётся за имя объекта.

Cold completion готовит объявленную family через обычный input controller.
Если receiver зависит от ещё холодного результата `Find`, native AST discovery
проверяет обращения к настоящему контейнеру в той же процедуре или функции.
Shadowing, комментарий или обращение в другом теле не разрешают чтение family.
После вывода reference receiver применяется существующая адресная подготовка
его metadata owner. Произвольный arm объединения типов не выбирается.

## Current input и композиция

Обычный критерий явно задаёт `Type` и `Content`; заимствованный может их не
передавать. QName-aware `Content` принимает пути обычного/стандартного реквизита
и реквизита табличной части поддержанных ссылочных объектов. Неизвестные family,
неполные пути, дубли и malformed flags отклоняются адресно.

Заимствование требует того же имени и исходного UUID. Допускаются наследование
или равный поисковый тип и добавление состава без удаления базовых полей.
Иной поисковый тип, неверный UUID и unavailable/invalid видимый вклад не
подменяются базой, sibling или старой успешной моделью.

Несохранённый `Content` меняет поля результата без изменения BSL input.
Dirty authority, watch, invalid/retraction, repair, close и restart/replay
остаются у прежнего controller. Отказ native preflight передаётся через
существующий `analysis_composition_incomplete`. Отдельный syntax route доступен
при семантическом отказе.

## API и границы

Экспериментальные `CanonicalMetadata`, `MetadataListingData`, listing input и
native resolver API получили отдельную family. Это изменение source API без
compatibility layer. CLI/config/JSON schemas, версии, зависимости и состав
девяти локальных пакетов kernel не меняются.

Принят BSL container/Find с явным текущим составом. Собственный manager module,
формы и команды критерия, его SDBL virtual table и полная транситивная подготовка
не объявляются реализованными. Classic XML Configuration не получает второй
XML loader критериев. EF-18 и MVP остаются в работе.

## Проверки

Frozen native/adapter/SDK gate: 21 команда, включая fmt, check, strict Clippy,
strict rustdoc, default/all SDK tests и debug/release binaries. Kernel —
4425 passed / 25 ignored; adapter — 109; SDK — 226 default / 535 all-features,
без ignored. Все 1632 Rust/Cargo/config/locale inputs сохранились. Шесть
корневых gates предыдущего CommonCommand-среза переиспользованы по совпадению
всех 749 корневых inputs; корневой workspace не зависит от SDK/kernel.

Тесты проверили cold namespace, native Find methods, тип аргумента, массив ссылок,
shadowing, body-bounded discovery, current Content, watch, invalid/repair/close,
UUID/Type refusal в configured CFE и отдельный syntax route. Настоящий checker
на RU/EN вернул ожидаемые UnresolvedField/TypeMismatch и exit 1/0.

Explorer — 356 Node tests, без skipped. Первый прогон до client recovery дал
единственный сбой прежнего теста orderly shutdown; повтор без изменения кода
прошёл. Причина этого сбоя не установлена. Установленный прежний VSIX подтвердил
новые criterion completion/hover в CF, но общий прогон остановился на cleanup
импорта: серверный guard отозвал сеанс, watcher callback не пришёл.

Клиент теперь проверяет только объявленные сервером маркеры наблюдённого импорта
до их удаления. Ожидание отменяется при смене сеанса/проекта и shutdown; активный,
включая dangling symlink, маркер не разрешает reopen. Происхождение пропущенного
native события не установлено; исправлено зависание восстановления после него.
Новый VSIX содержит 55 совпадающих production modules и не содержит helpers.

С ним установленная матрица прошла current Content, native completion/hover,
dirty foreign base, invalid/repair, restart и import replay для CF/configured-CFE,
прежние native-CF consumers и syntax/refusal native-CFE. Все 23 дочерних процесса
завершились; 45/40/151 файлов и native-CFE sources сохранились.
Native Save, физический ввод и новый запуск платформы 1С не проверялись.

Readonly проверка большой native конфигурации дала ready для обоих критериев:
по одному modeled descriptor, без parse issues. Большая композиция остановилась
на FunctionalOption: cold 15.52 s, peak RSS 391446528 bytes. Исходники сохранились,
процессы завершились. Это отказ, не приёмка производительности; фактическая
совместимость большой CF/CFE пары ещё не подтверждена.

[Машинные результаты](analyzer-filter-criteria-results.json).
