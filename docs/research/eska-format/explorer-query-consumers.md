# Потребители полей запроса в установленном Explorer

2026-10-15: ESKA SDK `083facd`, Explorer `7558a85`; версии не менялись.
[Структурированный результат](explorer-query-consumers-results.json) отделяет
Node/real-process проверки от установленного клиента.

Explorer согласует queryConsumers отдельно от queryAnalysis и использует тот же
diagnostic pull. Findings прямых DCS/DynamicList bindings относятся к физическому
resource из declaration, а не к строке запроса. XML сохраняет native language;
текущие тексты проходят прежние journal/ACK/replay. Проверяются root/composition,
settings/session/input stamps, hash/origin и precision. При pending/invalid/close,
watcher, restart и import guard прежние findings отзываются. Несколько queries
одного resource сохраняют независимые contributions.

Полный Node gate — 323 passed, без skipped/failed; TypeScript compile прошёл.
Real-process CF/configured-CFE RU/EN проверяет XML repair/invalid/close/restart,
неизменную query version, сохранность source bytes и завершение детей.

Свежий VSIX установлен в отдельный профиль VSCodium 1.109.51242 на Linux x86-64.
Три положительных сценария — generated CF, configured CFE и CF из выгрузки 1С —
подтвердили readonly Problems target, XML BOM/Unicode ranges, repair без изменения
query, invalid current без saved fallback, restart/import replay. Native CFE
сохранила explicit semantic refusal и отдельный доступный syntax profile.
Все 12 analyzer children завершились; source bytes сохранены, профиль удалён.

54 compiled modules совпали с VSIX, тестовые helpers исключены. VSIX 0.0.11:
1 500 947 bytes, SHA-256
`662803559a7fc9a852831c54a2400a9bc1fbe93c7c1b8cb46f324d2d583f2605`.
Analyzer SHA-256:
`36b59f4c2a619f5de99f5ec0013487aa8be3a6467585812739e4791798e514bc`.
Production sources не менялись между Node gate и installed приёмкой;
acceptance docs добавлены после упаковки. Helpers воспроизводятся через
`test/run-analyzer-host.mjs` с `ESKA_HOST_SLICE=query-consumers`.

Это native editor API без физического ввода и Save. Matching native base/CFE,
параметры, выражения/связи/итоги/настройки СКД и полный MVP ещё не приняты.
