# Formatting и проверки

## T26 — `eska fmt`

**Статус:** `PLANNED`  
**Зависит от:** T03, T22

Режимы `fmt`, `fmt <scope>`, `fmt --check`; project/object-aware scope,
детерминированность, скорость и одинаковый результат в CI. Не возвращать старый
экспериментальный formatter и не редактировать структурный XML regex-ами.

## T27 — `eska check`

**Статус:** `PLANNED`  
**Зависит от:** T26, T22; расширяется после T28

Агрегатор project, Designer XML, formatting, BSL diagnostics и VCS policy checks.
Интегрировать зрелые анализаторы (например BSL Language Server), не переписывая их.
Обязательны human и JSON output, предсказуемые exit codes и независимое выполнение
checks.

