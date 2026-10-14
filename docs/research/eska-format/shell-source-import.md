# Повторный импорт через основную оболочку

`eska import` определяет формат выбранного проекта. Для ESKA preview/write
выполняет совместимый `eska-build import`; Native/XML projection, journal,
receipt и recovery остаются в одном child. Прежний Designer XML маршрут и
его JSON не меняются.

```sh
eska --project-dir project import incoming.cf --format json \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" \
  --ancestor-sha256 "$CONFIRMED_ANCESTOR_SHA256"
eska --project-dir project import incoming.cf --format json \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" \
  --ancestor-sha256 "$CONFIRMED_ANCESTOR_SHA256" \
  --expected-plan-sha256 "$PREVIEW_PLAN_SHA256" --write --receipt recovery.json
```

## Выбор и authority

Default — preview; `--dry-run` допускается как явный preview. Запись требует
`--write --receipt`; `--receipt` без записи, `--force` для ESKA и комбинации
write/force/dry-run отказывают. Optional exact plan отклоняет изменившийся
preview. Base pin берётся из флага либо прежнего environment fallback; ancestor
требует независимого explicit digest. Локальная history не выбирает доверие.
Отдельный `--ancestor` указывает ранее подтверждённый raw store.

Выбор member выполняется прежним selector. Parent проверяет pin syntax,
совместимость exact version/protocol/type/source-import capabilities и явную
platform version. Обычное обнаружение платформы выполняет child; parent UI
используется только при `--select-platform`. Artifact/ancestor/receipt пути
разрешаются от первоначального `--project-dir` один раз; OS bytes сохраняются.
Relative ibcmd и child cwd не подменяются выбранным member.

## Потоки, ошибки и восстановление

Успешная делегация сохраняет stdin/stdout/stderr и numeric exit child; на Unix
signal exit следует shell convention. Parent не публикует второй JSON envelope,
не создаёт own journal/stage и не дублирует interrupt handler. Ошибка транспорта
получает локализованный parent code, доменные ошибки приходят от инструмента.
Windows numeric status не сокращается до u8; physical Ctrl+C проверяется отдельно.

Recovery receipt сохраняется независимо до source moves. При частичной записи
используется существующий `eska-project recover-import`; restart parent не
разрешает удаление source либо journal. Explicit source-only flags в XML-проекте
отказывают до native pipeline; несвязанный environment pin не меняет XML маршрут.

## Проверки

14 новых process groups и 18 старых XML scenarios прошли, включая all4/RU/EN,
preview/write/repeat, выбранный member/inheritance, stale plan, независимые pins,
absolute input paths, direct streams/exit/signals, siblings/PATH/oversized/timeout
probes и source-format boundaries. Три companion unit, check, strict Clippy,
fmt и locale parity прошли. Все 22 Rust gates прошли: root 862 unit /
646 all-feature integration, project CLI 69 tests, SDK 194/168/23, adapter 74/12.
Git-sensitive full tests выполнены без concurrent sandbox exec: защитные
parent .git masks иначе давали ложную bare-repository диагностику.

[Actual native матрица](shell-source-import-results.json) на 1С 8.3.27.2325
подтвердила all4 preview → exact-plan write → confirmed no-op repeat → build →
independent dump. Local BSL сохранился точно; support/settings не менялись,
receipt записана, journal завершён. Dump совпал с incoming control, кроме одного
отдельного local BSL comment. Windows, physical Ctrl+C и установленный client
restart/replay этой частью не принимаются.
