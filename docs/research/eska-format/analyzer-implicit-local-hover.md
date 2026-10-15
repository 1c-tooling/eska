# Hover неявных локальных переменных

Native positional card теперь принимает `SemanticSymbolKey::ImplicitLocal`.
Прежний resolver возвращал `Definition` только для явно объявленного local:
тип после `Value = GetFunctionalOption("Feature")` выводился, но карточка
`Value` без `Var` отсутствовала.

Карточка использует уже существующие native symbol identity, тип конкретного
вхождения и reaching declaration. SDK не создаёт типы или собственные BSL
карточки. Неизвестный native тип не публикуется как доказанный результат.
Объявление и UTF-16 ranges берутся из native source map, включая смену типа
при следующем присваивании. Выбор секций `type`/`definition` сохранён.

Проверка implicit identity выполняется для свободного имени раньше textual
resolver: local с именем platform global остаётся local. Поля receiver,
комментарии и присваивания из другого тела не становятся его объявлением.
Поддержаны procedure/function bodies и module code через тот же native API.
Новых dependencies, parser, config/JSON schemas и строк локализации нет.

Фикстуры функциональных опций в SDK и Explorer теперь используют обычное
присваивание без `Var`. Отдельная проверка сохраняет explicit-local contract.
Остальные ограничения [функциональных опций](analyzer-functional-options.md)
и незавершённые условия EF-18/MVP остаются в силе.


## Проверки

Kernel — 4428 passed / 25 ignored; adapter — 113; SDK — 233 default / 544
all-features, без ignored. Kernel/adapter прошли все десять fmt/check/strict
Clippy/test/rustdoc gates. Общий SDK прогон нашёл прежний тест, закреплявший
отсутствие implicit hover; он заменён проверкой typed kind и current range.
После этой правки повторены все 11 SDK gates, включая strict Clippy/rustdoc,
default/all tests и debug/release binaries. Десять gates переиспользованы
по совпадению всех kernel/adapter Rust/Cargo inputs. Все 1645 итоговых inputs
сохранились; шесть корневых gates переиспользованы по прежним 749 inputs.

Native тесты проверяют reaching assignment/type, BOM/CRLF, UTF-16 с non-BMP,
module code, shadowing имени platform global и изоляцию тел. SDK проверяет
явные/неявные locals, текущий storage, unknown/invalid retraction и CF/CFE.

Все 361 Node tests прошли без skipped. Установленный прежний VSIX подтвердил
hover неявного результата в CF/configured-CFE, dirty type, invalid/repair и
restart/replay; вся предыдущая query/metadata матрица также прошла. Все 25
children завершились, 57/49/151 файлов и native-CFE sources сохранились.
Native Save, физический ввод и новый запуск платформы 1С не проверялись.

[Машинные результаты](analyzer-implicit-local-hover-results.json).
