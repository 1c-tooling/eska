# Выборочная поставка компонентов

T97 — `IN PROGRESS`. Общая версия релиза не заменяет проверку контрактов.
`eska`, `eska-project`, `eska-check`, `eska-build` и `eska-vcs` отвечают на точный
машинный вход `__tool-contract` одной JSON-строкой, без project discovery,
других executables, сети, локализации и native probes:

```json
{"schema_version":2,"tool":"eska-project","target":"x86_64-unknown-linux-gnu","version":"0.14.3","config_schema":"designer-xml-1","bridge_schema":1,"metadata_api":{"major":1,"minor":12}}
```

У остальных четырёх инструментов `metadata_api: null`. `config_schema` обозначает
совместимую семью `eska.toml`, а не новый ключ внутри пользовательского manifest.
`bridge_schema` относится к внутренним process-запросам; Metadata API согласуется
отдельно. Приёмник требует точную identity, полный schema-2 документ, стабильную
версию, поддерживаемые config/bridge schemas и project API 1.12 либо более новый
minor той же major. Неизвестная major или неполный ответ не дают совместимость.
Проверка будущего minor не обещает поддержку неизвестных возможностей.

Существующие release automation, Actions и опубликованные установщики этим
изменением не переведены на компонентную поставку.

## Архивы и manifest

Фактический compilation target добавлен в контракт schema 2. Первоначальная
schema 1 без target не принимается установщиком. ZIP, release index и receipt
имеют собственную schema 1; эти номера не связаны с Metadata API.

`tools/package_components.py` создаёт по одному ZIP на выбранный инструмент,
`release.json`, `SHA256SUMS`, LICENSE и три Python-файла установщика. Имя архива:
`<tool>-<version>-<target>.zip`. Установка не требует Cargo, Git, Node или
оболочки `eska`; для этого bootstrap нужен Python 3.11+. Установленные Rust
инструменты от Python не зависят. Платформа 1С и bsl-analyzer поставляются
отдельно.

В ZIP находятся `package.json`, ровно один `bin/<tool>[.exe]` и `licenses/`.
Manifest содержит tool/version/target/contract и точный список файлов с size,
SHA-256 и executable-флагом. Отсутствующие, лишние, повторные, выходящие за
namespace, зашифрованные и symlink members отвергаются. Разрешены обычные
ZIP stored/deflate. Проверяются границы размера архива, manifest, числа файлов
и распакованных байтов. `release.json` связывает компонент с именем, размером,
SHA-256 архива и тем же контрактом.

В notices входят корневой LICENSE, найденные LICENSE/COPYING/NOTICE/COPYRIGHT
из закреплённого normal/build графа для target, перечень точных crates и
контрольная сумма Cargo.lock. Граф может содержать лишние build-компоненты;
он не объявлен точным списком linked code. Для MPL-зависимостей вложены точные
registry source archives с проверкой checksum. Если в crate нет текстов,
используются проверенные `tools/license-overrides/<crate>-<version>` из его
точного upstream commit; отсутствие комплекта останавливает упаковку.
Системные shared libraries не копируются в архив.

Упаковка выполняет native probe, поэтому запускается на совместимом host.
Входные executable должны быть собраны из текущих источников и Cargo.lock;
packager не пересобирает их и не доказывает provenance произвольной переданной
копии. Он сверяет стабильную версию и target всех выбранных бинарников,
проверяет их байты до и после чтения. Поддерживаемые имена target совпадают
с существующей dist-конфигурацией: Linux x86_64/aarch64 GNU и musl,
macOS x86_64/aarch64 и Windows x86_64 MSVC. Это перечень формата, а не отчёт
о приёмке всех сборок.

Локальная подготовка в новом собственном каталоге playground:

```sh
cargo build --workspace --bins --locked
stand_dir="$(mktemp -d "$(realpath ../eska-playground)/components.XXXXXXXX")"
python3 -B tools/package_components.py --bin-dir "$(pwd)/target/debug" \
  --output "$stand_dir/release" --component eska-project --component eska-check \
  --component eska-build --component eska-vcs --component eska
```

При одинаковых входных байтах и notices ZIP сохраняет постоянные timestamps,
порядок members и modes. Это свойство упаковки; оно не объявляет Rust/native
сборку бинарно воспроизводимой.

## Выборочная установка

Из подготовленного комплекта, с явным изолированным root:

```sh
python3 -B BUNDLE/components.py install --manifest BUNDLE/release.json \
  --component eska-project --root PREFIX --check
python3 -B BUNDLE/components.py install --manifest BUNDLE/release.json \
  --component eska-project --root PREFIX
python3 -B BUNDLE/components.py status --component eska-project --root PREFIX --format json
python3 -B BUNDLE/components.py remove --component eska-project --root PREFIX
```

Без `--root` используется `~/.eska`. Файл устанавливается в `bin`, notices —
в `share/eska/<tool>`, receipt — в `components/<tool>.json`. Другие инструменты
и пользовательские файлы сохраняются. Receipt не позволяет усыновить ручную,
Cargo или cargo-dist установку; даже `--force` не заменяет чужой файл.
Свой executable проверяется без установки:

```sh
python3 -B BUNDLE/components.py inspect --executable CUSTOM_BINARY --format json
```

Для обновления запускается тот же `install` с новым manifest и выбранным
компонентом. Обновление проверяет прежний способ установки, точные bytes всех
управляемых файлов и совместимость объявленных установленных компонентов.
Повтор того же архива возвращает `up-to-date` без замены. Понижение версии
и другая сборка той же версии требуют `--force`; это не отключает ownership,
checksums, target или compatibility checks.

`--check` читает архив и receipts, но не создаёт install root и не запускает
вложенный executable. Перед реальной заменой executable запускается из нового
owned staging; полный фактический контракт должен совпасть с manifest.
После публикации проверяются байты и контракт установленного executable.
Probe ограничен десятью секундами и 64 KiB на stream; отказ и отмена завершают
и reaped только его собственный процесс.

Можно указать `--archive FILE_OR_HTTPS --sha256 HASH` вместо release index.
HTTPS index требует `--manifest-sha256 HASH`; его архив также проверяется по
закреплённому checksum. Нет неявного latest, смены канала или автоустановки из
обычных project/check/build/VCS операций. Локальные index и архивы работают
без сети. Дефолтные опубликованные installers не менялись: наличие нового
локального формата не означает, что release assets уже опубликованы.

## Транзакция и восстановление

OS-lock `.eska-components.lock` сериализует установки; сам lock-файл остаётся
после завершения, блокировка освобождается ОС. Перед первой заменой в новом
`.eska-component-*` сохраняются точные backup-файлы и журнал. Ошибка копирования,
повреждённый результат после публикации и отмена возвращают прежнюю установку.
Удаление проходит тот же preflight и удаляет только файлы выбранного receipt.
Неизвестное содержимое даже внутри namespace notices сохраняется.

При невозможности rollback backups остаются, а ошибка содержит путь staging.
Новая установка не перескакивает через прерванную транзакцию. Явное восстановление:

```sh
python3 -B BUNDLE/components.py recover --root PREFIX --transaction STAGING --format json
```

Перед восстановлением проверяются весь журнал, границы записи, backup hashes
и все текущие файлы. Если после сбоя файл изменён внешним процессом, восстановление
отклоняется целиком; backups не удаляются. Журнал с уже записанным commit marker
требует только очистки staging. Полная устойчивость к потере питания и сетевым
файловым системам этой реализацией не подтверждена.

При запуске canonical managed executable проверяет schema/contract своего receipt;
оболочка проверяет также выбранный component receipt до делегирования. Это чтение
ограниченного manifest и проверка размера binary, без полного хеширования при
каждом открытии IDE. SHA-256 ownership проверяется установщиком и `status`
перед изменениями; ручная подмена binary вне установщика не считается принятой
управляемой установкой. Help/version и `__tool-contract` доступны до этого gate.

Текущий полный прогон: 902 Rust-теста и 13 Python-сценариев прошли, 12 Rust-проверок
ignored. Проверены обе локали, прямые executable и оболочка, read-only preview,
выборочные update/remove, offline-повтор, ошибки контрактов/путей/checksum,
rollback, конфликт восстановления и реальный SIGTERM с reap дочернего probe.
Реальная установка собранных архивов и матрица минимальных комплектов ещё
выполняются; Windows/macOS/Remote этим набором не приняты.
