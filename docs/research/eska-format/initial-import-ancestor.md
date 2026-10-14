# Initial ancestor нового проекта

Queries creation сохраняет полный initial source snapshot до публикации
`eska.toml`. `ImportedProject.initial_ancestor` и JSON `ancestor_sha256` возвращают
его candidate digest отдельно от `accepted_base_sha256`. Старые Family,
Separated и Readable creation APIs возвращают `None`; их CLI JSON не изменён.

## Хранение и подтверждение

Raw store находится в `.eska/source-import/initial`. Он использует существующий
content-addressed `ImportAncestor` формат: marker, полный набор exact blobs и
проверенный source projection. Новый state имеет отдельную форму:

```json
{
  "schema_version": 2,
  "origin": "initial",
  "source": "src",
  "ancestor": {"sha256": "<candidate-ancestor-sha256>"}
}
```

State не содержит transaction/recovery reference и не объявляет выполненный
reimport. Существующие опубликованные state/journal/receipt версии 1 сохранены.
Новый private bootstrap вызывается только внутри exclusively claimed creation
root без опубликованного manifest. Ошибка убирает только принадлежащую операции
часть; общий creation cleanup сохраняется.

`eska-build import` по-прежнему требует независимо переданный
`--ancestor-sha256`. Если `--ancestor` отсутствует, resolver выбирает initial
store либо опубликованный incoming store согласно state. Проверяются source
path, форма state, exact blob set, candidate digest и complete projection.
Нельзя использовать support pin как ancestor или наоборот. Текущие локальные
YAML/BSL правки не меняют сохранённый initial snapshot.

Первый import, включая no-op по native values, проходит обычный journal. После
публикации state версии 1 указывает на raw incoming, сохраняя локальные правки
только в working source. Повторный confirmed import использует прежний no-op
контракт. Rollback первого журнала восстанавливает exact initial state v2.
Local initial marker не даёт права на recovery или на поддержку.

## Перенос через Git

`TransportPlan` добавляет scoped правило для `.eska/source-import/**` вместе
с source и accepted base: `-text -filter -ident !working-tree-encoding`.
Initial blobs, raw incoming/history и state переносятся побайтно. Member rules
разрешаются относительно его собственного root; скобки и пробелы сохраняются.
Git info/global policy остаётся ответственностью VCS consumer, а независимый
digest проверяется при открытии store.

## Проверки

Три новые core matrix cases на четырёх типах проверяют раздельные pins, resolver
после локальных правок, первый import/first no-op, обычный journal, rollback и
отказ повреждённого state/store. Старые 13 journal/recovery cases проходят.
Queries process matrix на RU/EN проверяет восьмой data key `ancestor_sha256`,
human сообщение и отсутствие этого поля у прежних layouts.

Настоящий Git commit/clone для standalone/member сравнивает source, accepted,
initial/raw history/state files и protected XML. Parent filter, UTF-16 encoding,
ident expansion и autocrlf не изменяют эти authorities после scoped rules.

Все 22 Rust gates прошли: root 821 unit/610 default integration (612 на
all-features), 27 build/56 project process, SDK 187/161/19, adapter 70 + 12,
minimal/build-core/build-CLI и strict Rustdoc. 24 opt-in root cases пропущены.
Четыре новых native проекта прошли первый preview/write без `--ancestor` path
на 1С 8.3.27.2325, используя независимо переданный initial digest. State v2
перешёл в journal state v1; saved local BSL, accepted bytes/settings и ранее
собранные CF/CFE/EPF/ERF сохранились. Их raw initial не включал local comment.
[Обезличенные результаты](initial-import-ancestor-results.json).

Эта часть не меняет существующие проекты и не захватывает их текущие правки
как ancestor автоматически. Общие new/init/migration и MVP ещё в работе.
