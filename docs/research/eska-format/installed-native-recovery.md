# Восстановление прерванного native import в установленном Explorer

EF-25 проверена на Linux с 1С 8.3.27.2325: настоящий импорт CF/CFE/EPF/ERF
прерывается после переноса старого `src`, затем отдельная команда завершает
импорт или возвращает прежний состав. Это восемь standalone-сценариев в обычном
установленном VSIX; [результаты](installed-native-recovery-results.json).

## Реальная граница прерывания

В отдельных копиях native-origin проектов открыты dirty YAML/BSL/SDBL, включая
документы удаляемого owner. У выбранного native EPF запроса нет; искусственный
SDBL ему не добавлялся. Входящие артефакты и independent controls взяты из
[проверки удаления](installed-native-retirement.md).

Настоящий `eska-build import` сначала выполняет preview с независимо заданными
support pin и ancestor. Для записи передаётся сохранённый transaction digest.
Тестовый GDB observer останавливает собственный writer на успешном возврате
настоящего rename `src` → journal `previous-source`. Product hooks и подставные
guard/watch notifications не используются.

На этой границе старые исходники уже отсутствуют по прежнему пути. Существуют
настоящий guard, полный journal, candidate и backup. Сверяются raw SHA-256
journal receipt с preview transaction, working/output revisions, отдельная
write receipt и точные прежние bytes import state. Guard не выбирает доверенный
digest. Нативные subprocess уже завершились; это дополнительно проверяется по
всем потокам writer и `/proc` для уникальных путей его staging.

Test runner Explorer `test/run-analyzer-host.mjs` использует обычный deletion
manifest с `recoveryAction`, `recoveryCli` и `recoverySupervisor.python/script`.
IPC directory создаётся самим runner вне выбранного project root. Observer и
отдельный многопоточный probe находятся в `test/native-import-recovery/` того же
репозитория; они не входят в VSIX. Этот стенд требует Linux x86-64 и GDB.

Пока writer удерживается, реальный OS watcher переводит Explorer в `restart`.
Не запускается новый analyzer, не отправляются новые draft inputs и Save RPC.
В сумме по четырём типам на каждую action удерживаются 19 документов: сохраняются
URI, текст, editor version и protocol version; `canSave` выключен.

После этого GDB завершает именно свой остановленный writer. Проверены executable,
argv, cwd, kernel start time, held threads, same-inferior exit event, readable
pidfd, отсутствие процесса и освобождение inferior. Используется `gdb-kill`:
этот путь GDB не публикует номер terminal signal, поэтому `observedSignal` равен
`null`. Числовой сигнал из команды или текста GDB не выводится. Отдельный probe
с четырьмя реальными потоками подтверждает отсутствие продолжения после rename.

## Complete и RollBack

Настоящий `eska-project recover-import` получает независимо сохранённый digest
и явную action. Первая команда возвращает `resumed`, повтор той же action —
`unchanged`; source inventory и import state после повтора остаются прежними.

| Action | Форматы | Exact replay / обычные Save | Удалённые документы |
| --- | --- | --- | --- |
| `complete` | CF, CFE, EPF, ERF | 11 / 11 | 8 orphan drafts без ACK, replay и пересоздания файлов |
| `roll-back` | CF, CFE, EPF, ERF | 19 / 19 | Все 8 прежних bindings восстановлены и снова допускают Save |

После удаления guard происходит ровно один automatic replacement с прежним
support pin. В новый процесс уходит ровно один `didOpen` на каждый допустимый
draft с прежними text hash и version. Новые подключения начинаются только после
снятия guard; readiness и тот же replacement session сохраняются до конца Save.
При RollBack весь исходный source inventory и прежние bytes/отсутствие import
state сверяются до первого Save. При Complete проверяется независимый ожидаемый
удалённый состав. Settings, accepted base и входящий артефакт сохранены.

После reap удаляется только новый native staging этого writer: проверены его
PID-derived имя, dev/ino, отсутствие alias и активных native пользователей.
Старые staging, transaction journal, receipt и source-write lock не удаляются.
Во всех восьми случаях удалена одна временная база; baseline staging inventory
восстановлен до recovery и повторно проверен после Save.

## Проверки и граница приёмки

[SDK guard](analyzer-import-guard.md) дополнен regression для root/manifest batch
без marker event: import hint больше не теряется при отсутствующем `src`.
Settings changes сохраняются вместе с ним. RU/EN process cases и 17 root/SDK
gates прошли: root 1542/1663 tests, SDK 264/601, strict Clippy/Rustdoc и сборки.
У root остаются 24 ранее игнорируемых теста; у SDK ignored нет.

Explorer прошёл compile, typecheck, 425 Node tests без skipped и VSIX packaging.
Исполняемые входы и установленный пакет сверены по hashes; raw receipts и
неуспешные ранние проверки самого GDB-стенда сохранены локально. Эти попытки не
включаются в восемь принятых native cases.

С тем же SDK/VSIX повторены обычный all4 addition (11 Save), all4 Save (28
публикаций) и native CF/CFE matching completion/hover/diagnostics. Все 90
наблюдавшихся процессов завершены; финальная cleanup не потребовала сигналов.
Восемь запланированных остановок writer относятся к самому recovery-сценарию.
После всех запусков остались ровно четыре пользовательские базы: size, allocated
size, device/inode и mtime совпали с keep-планом, лишних `.1CD` нет. Свободно
около 242 GiB. Клиентская приёмка сохранена в commit `6e3287f`, SDK fix — `7a0e634`.

Принимается одна реальная граница `BackedUp` и обе recovery actions для четырёх
standalone типов. Остальные точки прерывания, workspace-матрица и общий workflow
EF-20 остаются открыты. Эта проверка не расширяет semantic completeness SDK,
пределы foreign/inline/legacy identity и матрицу ОС. EF-25 — `IN_PROGRESS`;
общая приёмка MVP EF-16 продолжается.
