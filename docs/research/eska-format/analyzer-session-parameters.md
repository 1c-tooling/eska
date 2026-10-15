# Текущие параметры сеанса

`SessionParameter` имеет собственную native модель, listing и per-name Salsa
query. Это значение глобального `ПараметрыСеанса` / `SessionParameters`, с UUID,
именем, составным типом и явным происхождением расширения. Фиктивные manager/MDO
не создаются. Одноимённый `&Parameter` запроса не получает тип параметра сеанса.

Адаптер читает текущий typed descriptor общего parser. Требуется полный capture
и присутствующий `Type`; пустой объявленный тип сохраняет native Unknown.
Составные альтернативы, qualifiers, DefinedType и AnyRef TypeSet используют
существующий type converter. Неизвестные типы, неверные QName/UUID и неполное
усыновление отказывают явно. Native XML reader этого нового вида проверяет
namespace и неоднозначность; прежние tolerant readers остальных видов сохранены.

Canonical registration остаётся authority даже при Pending/None/invalid:
сохранённый XML или прежний base value не подставляются. Состав имён хранится
отдельно от values. Lookup использует только base и зависимую цепочку выбранного
файла; unrelated sibling CFE исключена. Adopted значение обязано ссылаться на
первоначальный UUID, совпадения имени недостаточно. Duplicate имена не выбирают
случайный источник.

Cold completion сначала спрашивает native inference о типе receiver. Только
настоящий SessionParameters или его типизированный alias готовит объявленное
семейство через существующий input controller. Другие metadata families и BSL
bodies не загружаются. Локальная переменная/параметр с тем же именем не приобретает
configured fields. Переподготовка после overlay/watch/close сохраняет текущие
revision/hash, отмену и общую квоту. Композиционный route сохраняет собственную
подготовку всех видимых вкладов.

Configured fields остаются изменяемыми, но наследуют execution contexts настоящего
platform container из native PlatformData. В ThinClient/WebClient они недоступны
даже после полной подготовки metadata. Статический `Очистить` / `Clear` сохраняется.
Карточки `bsl-analyzer-reference.syntax_help` проверены 2026-10-15: контейнер и
метод доступны в thick client/server/external connection; `Clear` принимает
необязательный массив очищаемых параметров. Это reference/API проверка, отдельный
запуск кода параметров сеанса на платформе 1С не выполнялся.

Подключён седьмой локальный пакет `hir-def` той же закреплённой upstream revision.
Добавлены только существующему ConfigsDatabase два метода configured values;
новых внешних crate versions нет. Открытые Rustdoc ссылки исправлены без
подавления предупреждений. Экспериментальные `CanonicalMetadata`,
`MetadataListingData` и ConfigsDatabase расширены; JSON/config schemas прежние.

Проверки охватывают cold bilingual completion, aliases/shadowing, видимость
CF/CFE, неверное adoption, duplicate/pending slots, адресный churn, сохранение
соседей, overlay/watch/close, отмену, quota, client contexts и framed RU/EN.
Это принятый срез EF-18; остальные metadata families, общий memory budget и
полная семантика проекта остаются в работе.

Общий frozen native/adapter/SDK набор прошёл все 21 gate: fmt/check, строгие
Clippy/Rustdoc, tests и debug/release binaries. SDK — 213 default / 506 all-features,
adapter — 100, selected native workspace — 3310 passed / 21 ignored. Набор из
1502 Rust/Cargo files совпал до и после проверок и build. Lockfile package versions
совпали с прежними; `hir-def` изменил источник на локальный того же upstream pin.
