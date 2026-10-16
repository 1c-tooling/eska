# Native import выбранного workspace member в установленном Explorer

На Linux и 1С 8.3.27.2325 пройдены 20 случаев: addition, deletion, rename и обе
действительные recovery actions для CF/CFE/EPF/ERF.
[Результаты](installed-native-workspace-results.json) относятся к обычному
установленному VSIX и прежним проверенным бинарникам.

## Выбор проекта и изоляция соседей

Каждый workspace содержит пять физических members: все четыре типа и копию
выбранного типа. У копии совпадают native root UUID, module owner UUID, source
bytes/modes и independently approved pin; имя проекта и физический root другие.
Это проверка изоляции members, а не semantic композиции CF/CFE.

Анализатор запускается с workspace root и точным именем проекта. Его hello
показывает физический member root и `scope.name`; native preview/receipt
используют member root и `scope.project`. Обычный импорт вызывается через
`eska --project-dir <workspace> import ... --project <name>`.
Recovery намеренно адресует точный member через `eska-project recover-import`.

Версия платформы наследуется из workspace `[build]`; CLI override отсутствует.
120 фактических inspect-проверок трёх исходных compositions и пяти members
прошли на RU/EN с одинаковым JSON. Первые 40 checks сохранены отдельно.
Соседние members, workspace manifest/settings и контрольный файл в `dist`
сохраняют bytes, modes и mtime. Контрольный файл не объявляется native build.

Перед импортом Explorer действительно выбирает соседний member, открывает
guarded BSL и подтверждает его несохранённый текст собственным SDK ACK.
Затем настройка проекта переключает Explorer обратно. Барьер наблюдения
ставится **до** переключения: новый выбранный host не получает соседние
`didOpen/didChange` и Save RPC, хотя UUID и исходные bytes совпадают.
Соседний документ сохраняет exact текст, editor/journal versions и URI, теряет
ACK/write authority и остаётся открытым dirty до явного discard в cleanup.
Это проверено для всех 20 случаев без записи черновика на диск.

## Публикация, черновики и восстановление

В выбранном member проверяются dirty YAML/BSL и declared SDBL; у native EPF
этого набора query отсутствует. Все 20 сценариев прошли один automatic
restart/replay без ручного переподключения. После разрешения публикации
выполнены 63 обычных Save с точными bytes под прежним accepted pin.

Deletion и rename сохраняют 16 исчезнувших dirty вкладок без replay/Save и
пересоздания файлов. Rename открывает восемь новых URI с прежними owner UUID
без переноса старого текста. Четыре Complete добавляют ещё восемь orphan drafts;
четыре RollBack восстанавливают source/import state и восемь прежних bindings.

Восемь настоящих native writers прерываются в `BackedUp`: старый `src`
перенесён, candidate готов, guard/journal удерживаются в выбранном member.
Writer cwd остаётся workspace root. GDB подтверждает прекращение того же
inferior, readable pidfd и reap; числовой SIGKILL не заявляется.
Обе recovery actions и их повтор проверяются отдельной командой по
независимому transaction digest. Пока guard удерживается, тексты/версии
сохранены без новых document inputs/Save RPC; replacement подключается после
снятия guard. Временные native базы остановленных writers удалены после
проверки точных identities и завершения native процессов.

## Проверки и границы

Explorer прошёл compile, typecheck, 425 Node tests без skipped и VSIX packaging.
Сверены 184 входных файла клиента, 58 production JS и manifests/locales пакета.
Rust/SDK здесь не менялись: повторно сверены 1694 исходных входа, шесть
бинарников и receipts 17 ранее выполненных gates из
[предыдущей приёмки](installed-native-recovery.md).
Повторный запуск этих Cargo gates этой частью не заявляется.

С тем же пакетом повторены all4 guarded Save (28 публикаций) и native CF/CFE
matching completion/hover/diagnostics. Все 164 наблюдавшихся процесса завершены;
финальная cleanup не потребовала сигналов. Восемь запланированных остановок
writer входят в recovery-сценарии. Клиентская проверка сохранена в commit
`75b634e` — `test(explorer): Проверена изоляция member при импорте и восстановлении`.

После всех запусков в playground остались ровно четыре пользовательские базы:
size, allocated size, device/inode и mtime совпали с keep-планом; лишних
`.1CD` нет. Свободно около 242 GiB. Ранее удалены 182 собственные тестовые базы
и 16 generated directories, освобождено около 8.3 GiB; пользовательские demo
и базы сохранены.

Установленный клиент принимает только границу `BackedUp`. Core отдельно
проверяет `prepared/backed-up/installed/state × Complete/RollBack` с повтором;
DoD EF-25 не требует installed-прерывания каждой journal-фазы.
Эта часть не расширяет semantic completeness, foreign/inline/legacy identity
или матрицу ОС. Явный повторный импорт нового заимствованного CFE owner ещё
требует отдельной проверки. Общий CLI workflow EF-20 и итоговая приёмка MVP
EF-16 продолжаются; EF-25 остаётся `IN_PROGRESS`.
