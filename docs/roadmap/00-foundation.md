# Foundation

## B00 — Clean baseline

**Статус:** `DONE`  
**Результат:** старые экспериментальные `fmt`, formatter, async dispatcher и
неиспользуемые dependencies удалены. Остался минимальный `clap` CLI с работающими
`--help` и `--version`.

## B01 — Локализация CLI

**Статус:** `DONE`  
**Зависит от:** B00

Реализованы:

- встроенные Fluent-ресурсы `ru-RU` и `en-US` с parity-тестом;
- `Locale`, `Localizer`, параметризованное форматирование;
- приоритет `--lang` → `ESKA_LANG` → system locale → `en-US`;
- bootstrap чтение `--lang` до полного parsing `clap`;
- полностью локализованный help и CLI integration tests.

Строка использования в `--help` / `-h`: `eska [ПАРАМЕТРЫ]` для `ru-RU` и
`eska [OPTIONS]` для `en-US`.

Известная граница: diagnostics ошибок parsing пока формирует `clap` на английском.
