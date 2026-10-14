# Создание ESKA через основную оболочку

`eska new --source-format eska` создаёт минимальные CF/CFE/EPF/ERF либо новый
проект из нативного артефакта. Designer XML остаётся default; прежние команды
и их вывод сохраняются. Source операции выполняет `eska-project`, native unpack —
`eska-build`. Оболочка отвечает за policy, workflow, Git и workspace enrollment.

```sh
eska new demo --source-format eska --type configuration --workflow trunk --no-vcs
eska new report-demo --source-format eska --type report --workflow trunk \
  --metadata-name НовыйОтчет --initial-version 2026.10.0.1 --format json
eska new imported-demo --source-format eska --from delivered.cf --workflow trunk \
  --platform-version 8.3.27.2325 --distrobox 1c-ubuntu-env --no-vcs --format json
eska --project-dir workspace new report --source-format eska --type report --format json
```

## Политика и инструменты

No-TTY/JSON требуют явного standalone type/workflow для пустого проекта;
`--from` определяет фактический тип по нативному входу. Для member workflow
принадлежит workspace. Схема версии проверяется до публикации: без явной версии
пустой member получает `0.1.0` либо `0.1.0.0` по inherited scheme. Несовместимая
явная версия отказывает. Native input сохраняет Name/UUID/version; metadata-name
и initial-version относятся только к пустому new.

Общий concrete companion helper ищет sibling, затем PATH; найденный несовместимый
sibling не заменяется другим tool. Проверяются exact package version, protocol 1,
tool/envelope/command и потребляемые capabilities. Probe ограничен временем и
stdout; неуспешный preflight не создаёт staging/member parent/destination.
Для пустого new платформа не запускается. Native `--from` явно выбирает платформу
через прежний resolver и передаёт runner options без shell interpolation.

Child `eska-project` исключительно захватывает final root. Native unpack работает
в private staging с отдельной точной проверкой результата и его XML identity.
Actual type/version и source bytes проверяются перед source import и parent
finalization; физический stem EPF/ERF не принимается за metadata Name.
Compound capture дожидается child completion, ограничивает retained stdout и
полностью drain-ит pipe. Parent сохраняет control над своими transient данными.

## Receipt и parent finalization

Direct Queries import добавляет `manifest_sha256` к прежнему receipt; Family/
Readable JSON остаются прежними. Hash относится к исходной serializer строке
до publication. Parent проверяет точный root/source/name/layout, typed candidate
pins и оригинальный manifest, затем actual source/accepted/initial identity.
Saved source revision должна совпасть с initial snapshot. Эта проверка не
выдаёт support approval и не создаёт XML generation.

Workflow manifest меняется expected-byte publication. New standalone Git
использует exclusive `.git` reservation и существующий gix initializer.
Workspace enrollment сохраняет исходные root policy bytes, публикует точный
member и проверяет общую discovery; чужие изменения не перезаписываются.
Опубликованный child root никогда не удаляется parent cleanup.

Ошибка после child creation возвращает retained root, stage, candidate pins,
`onboarding_complete = false`, `support_approved = false`,
`destination_removed = false` и recovery instruction. Она не выдаёт
незавершённый enrollment/Git за успех. Final success JSON использует typed
проверенные значения; raw child data не копируется в него произвольно.
Новый shell envelope — schema 1/tool eska/command new. Child stderr передаётся
напрямую, stdout разбирается как receipt; промежуточный success не печатается.
Неуспешный child exit/signal входит в диагностику compound операции, которая
возвращает собственный error exit.

## Проверки

12 новых process groups покрывают пустые/native all4 × RU/EN, human/JSON/no-TTY,
workspace обеих схем версии, flags, sibling/PATH/missing tool, bounded probes,
изменённые manifest/source/accepted/state/staging, foreign Git, failed acknowledgment
и policy race. Адресные unit/strict Clippy и 10 старых XML new cases прошли;
legacy import/Git cases и все 22 Rust gates прошли в разрешённой среде.
[Native матрица](shell-project-creation-results.json) подтверждает восемь
empty/from циклов четырёх типов на 1С 8.3.27.2325 с независимыми build/unpack,
identity/version и exact source/base/history/settings. Positive CFE from использует
versioned minimal artifact; CFE с пустой версией отдельно отказан до destination.
Windows, физический Ctrl+C и установленный IDE-client здесь не принимаются.
