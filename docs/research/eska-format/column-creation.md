# Создание колонок существующей табличной части

Двадцать первая часть EF-24 разрешает новый inline `Attribute` в принятой
`TabularSection`, которая непосредственно принадлежит `Catalog` или `Document`.
Используются общий protected selector и прежние `prepare`/`build`; новых команд
или параметров нет. Поддержаны обе descriptor schemas и layouts 1/2/3.

## Профиль и полномочия

Новый элемент добавляется в `children` существующей табличной части. Его форма
совпадает с [минимальным реквизитом](attribute-creation.md): ровно
Name/Synonym/Comment/Type, без собственных namespaces, attributes, children,
InternalInfo или preserved XML. Существующие generated identities табличной
части остаются принятыми native данными. Создание самой табличной части не принято.

Владелец определяется по принятому UUID и прямому parentage. Изменение рабочего
class/UUID или перенос чужого UUID не создаёт полномочий. Namespace context
собирается из принятого XML envelope, root и самой табличной части, включая
локальное shadowing. Рабочие служебные поля не заменяют этот контекст.
Новый UUID остаётся `unknown_object` до отдельного принятия базы.

`permission(table, Create)` требует допустимого принятого родителя и прежнего
supplier/freeze permission. Native правило поставщика остаётся отдельным для
каждого UUID: запрет корневого объекта сам по себе не запрещает его разрешённую
табличную часть. Дополнительная ESKA freeze наследуется от конфигурации,
корневого объекта и табличной части.

Старые колонки остаются ровно один раз и в прежнем относительном порядке.
Каждый сдвигаемый член требует собственного Reorder permission. Append после
неподвижной frozen колонки допускается; её сдвиг исключает вставку целиком.
Неизвестный preserved child сохраняет весь принятый состав.

Имена колонок проверяются в отдельной коллекции табличной части: Unicode
identifier и отсутствие совпадений после `to_lowercase`. Имя табличной части
и имена реквизитов корня не входят в эту коллекцию. Проверка не заменяет все
ограничения стандартных имён и длины на платформе.

Type имеет один редактируемый источник и явные native slots. Непустой профиль
принимает XML Schema Boolean/String/Number/Date и именованные Type/TypeSet
current-config; QName и slots проверяются в действительном контексте родителя,
именованные цели — в окончательном declared составе. Полные ограничения типов
и qualifiers проверяет 1С; профиль существующих колонок не сужается.

## Отказы, чтение и координаты

Запрещённая вставка использует прежнюю operation `{"kind":"create"}` с UUID
реальной табличной части или запрещённой сдвигаемой колонки. Strict отказывает
до generation; `--allow-exclusions` исключает новые колонки и сохраняет
независимые разрешённые свойства. Некорректные разрешённые профили, имена,
UUID/QName/Type targets отказывают и в режиме восстановления.

Добавление вместе с удалением, rename или перестановкой старых колонок
не принимается. Создание табличных частей, новые standalone descriptors,
остальные владельцы и автоматическое изменение BSL/форм остаются за границей
этого профиля.

Saved/overlay All/Summary читают новые колонки из общего YAML. Source map
использует действительный родительский namespace context; новый UUID/Name/Type
относятся к Working и требуют checksum точного рабочего текста и совпадения
рабочего и выбранного native узла. Принятой координаты нового узла нет.

Рабочие файлы, source manifest/hashes, settings, accepted storage и CI pin
сохраняются. Schema/rules/tool protocol versions не менялись; экспериментальный
Create permission расширен для принятой табличной части. Отдельной CST-команды
создания нет: новый узел редактируется вручную до принятия UUID.

## Проверки

Установленная 1С 8.3.27.2325 до реализации приняла восемь минимальных native
колонок существующих табличных частей двух владельцев с Boolean/String/Number/Date
и дополнила стандартные свойства при выгрузке. Инвентаризация C1 содержит
532 колонки в 114 табличных частях справочников и документов.

Прошли 16 root/14 SDK/5 adapter gates и обе release-сборки: fmt/check,
strict Clippy, default/all-features tests, Rustdoc и minimal core/project-cli/build-cli
boundaries. Root — 533 unit + 565 integration, 24 opt-in пропущены; minimal
project — 340 unit + 33 CLI. Добавлены восемь core, один supplier integration
и два CLI tests. Проверены оба envelopes, пустая таблица, supplier/freeze,
сдвиг колонок, collision/profile/Type targets, неизвестные slots, неподдержанный
родитель, сочетания с удалением/rename/reorder, сохранность и повторное поколение.
Локальный namespace context и Working coordinates проверены отдельно; ru/en JSON
одинаков. SDK — 359 tests; adapter — 70 unit + 12 integration.

Реальная C1: двенадцать колонок/два YAML совпали с независимым native контролем
по 8 778 файлам/8 077 каталогам без exclusions. Проверены Boolean/String/Number/Date,
EnumRef и String/Number composite, английские/кириллические имена и localized
Synonym. Неподвижная frozen колонка допускала append; её сдвиг отказал в ru/en
до generation. Recovery сохранил новые колонки другого владельца и совпал
с отдельным native контролем. Release project CLI работал с PATH без платформы,
Git, оболочки и без machine config. Все 3 595 readable YAML прошли schemas;
3 595 descriptors/8 342 объектов равны в saved/overlay All/Summary.

Обе CF собраны и повторно выгружены через 1С 8.3.27.2325: 8 769 файлов/
8 064 каталога совпали кроме 7 011 configVersion. Новые UUID, имена, type alternatives
и qualifier values сохранены на обеих routes; generated identities табличной
части и порядок старых колонок проверены отдельно. В составном Type платформа
ставит NumberQualifiers перед StringQualifiers; исходный порядок до сборки
сохраняет writer. Стандартные свойства материализует платформа. Artifact checksums
верны, версия проекта, sources/accepted/native/settings сохранены.
[Обезличенные результаты](column-creation-results.json).

EF-24 остаётся `IN_PROGRESS`: прочие structural/compound profiles ещё нужны.
Клиент, обновление рабочей ИБ, полный BSL analysis и новые CFE/EPF/ERF этой частью
не приняты.
