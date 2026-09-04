# Formatting и проверки

## T23 — Спецификация test backend

**Статус:** `PLANNED`
**Зависит от:** T19, T20

До реализации `eska test`, `test --affected` и CI определить проверяемый
контракт: поддерживаемые test frameworks/runners 1С, discovery и стабильный ID
теста, выбор environment, фильтрацию по affected objects, human/JSON result,
exit codes, credentials и одинаковое локальное/CI поведение.

Результат T23 — отдельная принятая спецификация и декомпозиция implementation
tasks. Код команды, собственный test runner и новая dependency в T23 не входят.

## T26 — `eska fmt`

**Статус:** `PLANNED`  
**Зависит от:** T03, T19

Режимы `fmt`, `fmt <scope>`, `fmt --check`; project/object-aware scope,
детерминированность, скорость и одинаковый результат в CI. Не возвращать старый
экспериментальный formatter и не редактировать структурный XML regex-ами.

## T27 — `eska check`

**Статус:** `PLANNED`  
**Зависит от:** T19, T26; расширяется после T28

Агрегатор project, Designer XML, formatting, BSL diagnostics и VCS policy checks.
Интегрировать зрелые анализаторы (например BSL Language Server), не переписывая их.
Обязательны human и JSON output, предсказуемые exit codes и независимое выполнение
checks.
