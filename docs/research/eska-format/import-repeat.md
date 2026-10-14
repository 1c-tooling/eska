# Повтор импорта без перезаписи исходников

EF-25 теперь отличает новую raw выгрузку от новых данных проекта. При одинаковой
карте и точных owner/module/query/payload bytes изменение только
`ConfigDumpInfo.xml` и input fingerprints карты не требует новой публикации.
Это проверка точных данных, не широкая semantic equivalence XML.

После уже завершённого импорта план возвращает `LayoutPublication::Unchanged`,
если selected source совпадает с working и incoming data совпадают с отдельно
подтверждённым предком. Исходники, timestamps, settings, support и журнал не
переписываются. Сохранённый raw ancestor остаётся прежним; fresh CDF не назначает
новый common source. Полный preview incoming bytes остаётся доступен.

`ProjectSourceImportPlan::resulting_ancestor_reference()` показывает pin,
который будет действовать после успешной публикации. Его нужно использовать для
следующего `project_import_ancestor`; `source_plan().incoming_reference()`
по-прежнему описывает точный captured input и может отличаться при no-op.
Receipt/schema, локальный state и CI pin не меняются ради совпадения references.

Первый импорт сохраняет подтверждённого предка, даже если selected bytes уже
совпали с working. Новые данные, совпадающие с локальными правками, также
обновляют ancestry: сравнение будущего импорта должно начинаться с них.
Permission/conflict checks и rechecks всех snapshots выполняются перед no-op;
изменение native input после preview не считается успешным повтором.

## Проверки

Существующий CDF unit-сценарий дополнен first apply, новым service dump,
двумя no-op публикациями, проверкой полного project inventory и stale native
input. Отдельный unit проверяет изменение общего metadata ancestor при равных
working/output bytes. Native consumer повторяет четыре настоящих ранее
собранных CF/CFE/EPF/ERF с сохранёнными независимыми pin. Результаты и проверки
сохранены в [JSON](import-repeat-results.json).
Повтор того же artifact дал ту же raw incoming reference во всех четырёх случаях.
Дополнительно в owned complete CF/CFE XML изменён только service configVersion:
captured reference стала другой, resulting reference и source/history остались
прежними. Это отдельная контролируемая service variation, не новая native поставка.

Новая top-level карта, native CLI lifecycle и координация unsaved IDE tabs
остаются в EF-25. No-op не сокращает распаковку/валидацию платформой и не
объявляет непроверенные client/runtime/production profiles готовыми.
