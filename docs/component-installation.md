# Выборочная поставка компонентов

T97 — `IN PROGRESS`. Общая версия релиза не заменяет проверку контрактов.
`eska`, `eska-project`, `eska-check`, `eska-build` и `eska-vcs` отвечают на точный
машинный вход `__tool-contract` одной JSON-строкой, без project discovery,
других executables, сети, локализации и native probes:

```json
{"schema_version":1,"tool":"eska-project","version":"0.14.3","config_schema":"designer-xml-1","bridge_schema":1,"metadata_api":{"major":1,"minor":12}}
```

У остальных четырёх инструментов `metadata_api: null`. `config_schema` обозначает
совместимую семью `eska.toml`, а не новый ключ внутри пользовательского manifest.
`bridge_schema` относится к внутренним process-запросам; Metadata API согласуется
отдельно. Приёмник требует точную identity, полный schema-1 документ, стабильную
версию, поддерживаемые config/bridge schemas и project API 1.12 либо более новый
minor той же major. Неизвестная major или неполный ответ не дают совместимость.
Проверка будущего minor не обещает поддержку неизвестных возможностей.

Установщик и архивы разрабатываются следующим логическим изменением.
Существующие release automation, Actions и опубликованные установщики этим
изменением не переведены на компонентную поставку.
