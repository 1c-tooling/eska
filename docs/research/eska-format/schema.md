# Экспериментальная схема источника, EF-02

Это реализованный внутренний контракт `project::source_format`, ещё не source
format `eska.toml` и не публичная команда конвертации. `schema_version = 1` и
`rules_version = 1` экспериментальны. Неизвестная версия отклоняется до записи;
автоматическая миграция не выполняется.

Версия descriptor не совпадает с версией обратной карты. EF-24 добавляет
[карту версии 2 и самостоятельные файлы](descriptor-layout.md): `document`
сохраняет descriptor версии 1, `nested` пуст. Семейный источник версии 1
по-прежнему читается; явная миграция не запускается при открытии.
[Карта версии 3](resource-layout.md) добавляет явный payload source path;
descriptor и conversion rules остаются версии 1 в этой части. [Читаемая оболочка](readable-layout.md)
добавляет descriptor schema 2 поверх той же карты версии 3; rules не меняются.
Версии 1/2 имеют строгие отдельные wire schemas и одну lossless модель.

[Карта версии 4](source-manifest-v4.schema.json) связывает metadata descriptor
и native dialog с [единым form.yaml](managed-form-project.md): readable
descriptor schema 2 и optional body schema 2 в `ObjectSource.form`, роль
`managed_form` с native descriptor identity. [Combined schema](managed-form-source.schema.json)
не заменяет прежние схемы. Новый импорт доступен как opt-in core API;
[protected export/source map 3](managed-form-protected.md) подключены к принятой
identity. [Явная миграция](managed-form-migration.md) переносит source, pinned base,
standalone проект или workspace в map 4. Публичный CLI import остаётся задачей EF-20.

[Карта версии 5](source-manifest-v5.schema.json) добавляет
[query resources и единственные plain `.sdbl`](query-project.md). Связь с native
slot остаётся в resource XML-ссылке; карта объявляет resource/descriptor и полный
набор файлов. Map 5 сохраняет descriptor/body schemas и rules, доступна через
opt-in core import/export/reader. [Query protection/source map 4](query-protected.md)
подключены; [query editor](query-edit.md) использует accepted ownership и общий
conflict-checked writer. [Явная query migration](query-migration.md) переносит
source/base/project/workspace; прежние layout targets не понижают map 5.

[JSON Schema](descriptor.schema.json) описывает один descriptor. Семантические
ограничения дополнительно проверяет `Descriptor::validate`: уникальность UUID,
секций, свойств и language keys, корректность QName-контекста и opaque XML.
Проверяемый [пример configuration.yaml](configuration.example.yaml) использует
структуру обезличенного корпуса; это не самостоятельный проект для сборки 1С.

Документ содержит версию схемы/правил, XML-профиль и объект. Это отдельные значения
от свойства `Version` проекта 1С. Последнее является точной строкой: сохраняются
три/четыре компонента, ведущие нули и неподдержанные для bump номера.

У объекта есть `kind`, native `uuid`, namespace-контекст, дополнительные XML
attributes и упорядоченные `sections`. Секция `properties` хранит typed и preserved
свойства, `children` — inline части и явные ссылки на отдельные descriptors;
`preserved` — остальное содержимое в исходном slot. Для основных объектов
предусмотрен `object.yaml`, для корня — `configuration.yaml`.
[Импорт EF-03](import.md) объединяет отдельные описатели в документ владельца;
обратная сборка целого XML-дерева принимается в EF-04.

Первый typed-набор EF-02: `Name`, `Comment`, `Version`, `Synonym`. Начальная
часть EF-24 добавляет [28 скалярных свойств](typed-scalars.md) с проверкой класса
владельца. Значение имеет состояние `text`, `empty`, `nil`, `localized`,
`boolean`, `integer` или `enumeration`. Отсутствие — отсутствие property entry,
а не `nil`/YAML null. `text` непустой; пустое значение задаётся через `empty`,
поскольку отдельный пустой text node не восстанавливается из XML.
Пустая localized sequence сохраняется как пустая коллекция.
Составные значения вне принятого набора сохраняются opaque. Boolean хранится
YAML boolean, целое — точной строкой ASCII-цифр без ограничения размером машинного
числа. Enum проверяется по домену конкретного свойства. Общие размеры и связи
между свойствами пока проверяются платформой, не writer.
В них не выводятся новые значения по умолчанию. Структурные изменения остаются
неподдержанными до собственного плана и проверки обратных правил.

Непрозрачный фрагмент хранит исходный XML и локальные namespace bindings, включая
префиксы в значениях QName. Общие bindings наследуются от XML-профиля и владельца,
а не дублируются у каждого реквизита/свойства. UUID внутри `InternalInfo`
не пересоздаются. Сам opaque-блок
не считается новым классом редактируемых метаданных: он не может дублировать
typed секцию, известный дочерний класс или QName уже имеющегося свойства.
Экспорт обязан восстановить его контекст даже при отличающемся namespace родителя.

Identity источника строится на родном UUID и контексте владельца/проекта;
path-based `ObjectId` существующего IDE API не становится persistent identity.
Потребитель получает typed kind, собственный UUID, порядок, declared children и
значения; полная source-модель сохраняет данные, которых нет в семантической
проекции. Входная ревизия вычисляется по прочитанным байтам, не редактируется
полем YAML. Координаты YAML получаются из CST, а координаты исходного XML — при
импорте. Индекс и source map производны, не являются второй моделью хранения.

YAML — один UTF-8 документ без BOM. Открытие BOM даёт отдельную диагностику;
перевод старого файла в профиль выполняется явной операцией. Дубликаты ключей,
сложные/non-string keys, anchors, aliases, merge keys, tags, directives и folding
отклоняются. Literal строки допустимы. Создание выдаёт LF, существующий CRLF
читается; адресное сохранение оформления проверяется отдельно в EF-07.
Строки сериализуются с кавычками, точные числа и даты не проходят через float.

Для CST выбран [yaml-edit 0.3.2](https://docs.rs/yaml-edit/0.3.2/yaml_edit/)
без default features. Используется публичный lexer и CST API; исправления
upstream не копируются в ESKA. Принятие библиотекой отделено от строгой проверки
профиля по [YAML 1.2.2](https://yaml.org/spec/1.2.2/).

Core не зависит от CLI/LSP/DAP, Git и платформы. Ошибка возвращает стабильный code
и schema coordinates; human-представление добавляет будущий CLI/IDE-потребитель.
