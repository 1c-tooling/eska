# Проекция описания формы и её данных

`eska-bsl-adapter::form` принимает общий `ParsedDescriptor` и явно переданный
`bsl_metadata::Form`. Адаптер поддерживает `Form` и `CommonForm`, не читает диск
и не материализует основной XML descriptor.

## Источники данных

Имя, UUID и тип формы берутся из текущего описания YAML. Переданный ресурс не
может заменить identity описания. Элементы с их ID, parent ID, kind и DataPath,
реквизиты, типы и колонки, event/command handlers сохраняются из переданной
модели без перестановок и удаления повторов. Модель передаётся по значению.

Отсутствующий либо пустой `FormType` означает `Managed`, как в закреплённом
native reader. Известные значения распознаются тем же enum; неизвестное,
структурированное или дублирующееся свойство вызывает `AdapterError` с
координатами текущего описания. Summary captures, неполный descriptor, чужой
kind и неверный UUID не превращаются в готовую пустую форму.

В существующем ESKA-импорте управляемый dialog хранится как preserved
`Ext/Form.xml`, а основной и вложенные descriptors — в YAML владельца.
Проверочный пример `project_forms` читает только ресурс, сопоставленный
объявленной общей форме через [общий reader](form-dialog-reader.md). Он сравнивает полную модель с
[закреплённым native reader](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/xml_parser/form.rs).
Это исследовательский пример; bounded reader подключён отдельным изменением,
но версии ресурса, overlays,
source quota и публикация canonical form input ещё не подключены.

## Границы модели

Адаптер не подставляет dialog, элементы или default handlers. Caller должен
отличать отсутствующий обычный dialog от pending/missing/invalid объявленного
ресурса. Уже подготовленный `Form::ordinary_without_dialog` допустим только
как явное решение caller; binary dialog этим не разобран.

Проекция сохраняет существующую семантику `bsl-metadata`: это не полный
редактируемый формат управляемой формы. Native parser может пропускать элементы
без корректного имени/ID и представлять неразобранный тип как `Unknown`.
Полное равенство native модели не доказывает полноту описания UI или принятие
формы платформой 1С. EF-12 не начата, EF-18 остаётся `IN_PROGRESS`.

## Проверки

Пять новых unit-тестов проверяют источники identity, сохранность всего payload,
самостоятельное обновление dialog, обычные формы и ошибки с координатами.
Файловый тест импортирует descriptor/resource/module, сравнивает native модель,
проверяет saved/overlay/invalid/close YAML, другой dialog input и сохранность
BOM/CRLF. Основные XML descriptors в ESKA не появляются.

Все объявленные общие формы C1/C2/C3 совпали с native reader:

| Проверка | C1 | C2 | C3 |
|---|---:|---:|---:|
| Общие формы | 83 | 435 | 460 |
| Элементы | 1 649 | 9 341 | 9 688 |
| Реквизиты | 653 | 4 334 | 4 567 |
| Колонки реквизитов | 347 | 1 812 | 1 922 |
| Event handlers | 522 | 2 893 | 3 067 |
| Command handlers | 375 | 1 428 | 1 463 |
| Проверенные source/native/control files | 706 | 3 114 | 3 348 |

C1 содержит 84 preserved `Ext/Form.xml` при 83 объявленных общих формах:
лишний файл не активирует форму. Hashes всех проверенных файлов сохранились;
PATH пустой, платформа не запускалась. Вложенные формы проверены unit-тестом,
полное сравнение их моделей на реальных конфигурациях ещё не выполнено.

Все 6 adapter/5 consumer/15 root gates прошли: fmt/check, strict Clippy/Rustdoc,
tests; для root также проверена feature matrix. Адаптер — 69 unit/8 file tests,
общая сессия — прежние 279 tests, root — 899 passed/14 прежних ignored.
Native workspace, зависимости, версии и source/protocol schemas не менялись.
Результаты записаны в [машинном отчёте](form-adapter-results.json).
