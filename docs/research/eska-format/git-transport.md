# Сохранение байтов при переносе через Git, EF-10

В EF-09 стандартное `* text=auto` изменило CRLF в extensionless BSL-блоке
принятой базы. Поэтому одного хранения `accepted.json` и blobs в Git
недостаточно: checkout должен возвращать исходные байты.

`project_core::transport::TransportPlan` готовит точное добавление
к `.gitattributes` выбранного ESKA-проекта:

```gitattributes
# eska: preserve source and accepted bytes
"/src/**" -text -filter -ident !working-tree-encoding
"/.eska/accepted/**" -text -filter -ident !working-tree-encoding
```

Оба пути берутся из проверенного проекта. Правила относятся к его корню,
включая отдельный member workspace. Пробелы, Unicode, `#` и квадратные скобки
не превращаются в другие шаблоны. Перевод строк, filters, подстановка `$Id$`
и перекодирование отключены для выбранных входов. Настройки остальных путей
не меняются.

План сохраняет исходный `.gitattributes` целиком, включая BOM, комментарии,
CRLF и отсутствие последнего newline. Новые правила добавляются в конце;
повторная подготовка с теми же конечными правилами не делает новую запись.
Предпросмотр не меняет файл. Публикация существующего файла использует общий
expected-byte writer; создание нового — полную временную запись и exclusive
hard link. Изменившийся, read-only или перенаправленный путь не перезаписывается.
Файловая система без hard links возвращает ошибку; частичный файл не публикуется.
Чтение UTF-8 attributes ограничено 1 MiB. Сравнение перед rename не является
защитой от враждебного процесса в последнем окне записи.

Библиотека не запускает Git и не читает index, global/system attributes
или `.git/info/attributes`. По [документации Git](https://git-scm.com/docs/gitattributes)
последние строки одного файла переопределяют предыдущие отдельно для каждого
атрибута; вложенный `.gitattributes` и `info/attributes` могут иметь более высокий
приоритет. Их проверка относится к VCS-инструменту EF-19, а подготовка правил —
к project tool. План не является доказательством эффективной Git policy.
Защищённый exporter всё равно проверяет реальный набор файлов по отдельному pin.

Пустые native каталоги остаются в `source.json` и принятой базе. Git не хранит
пустые каталоги checkout; materialization восстанавливает их из закреплённого
состава без `.gitkeep` или дополнительных native файлов.

Шесть тестов проверяют exact-byte preview/publication, идемпотентность,
конкурирующие изменения, ограничения путей/чтения и symlinks. Реальный Git
проверяет C-style quoting, `autocrlf=true`, исходные text/ident/encoding/filter
rules и clean clone. BSL с BOM/CRLF, binary resource, accepted blobs, map и
generation сохраняются. Контроль с `info/attributes` действительно меняет
Git blobs; новый checkout не проходит protected prepare и не создаёт поколение.
System Git используется только как независимый oracle в тестах; библиотека
и будущий VCS-инструмент не получают новый shell backend.

Полный fmt/check/strict Clippy/test/Rustdoc и отдельные source/build gates
прошли: 338 unit, 529 integration; 144 unit в source core и 160 в build core.
14 opt-in тестов приложения и три в каждом минимальном ядре пропущены.

На отдельной полной копии C1 реальный clean clone с `autocrlf=true` сохранил
все 7 829 source files и 6 682 файла принятой базы. Независимый source-only
consumer после clone подготовил те же 8 778 native files и 2 645 owners;
pin, input/native revision, generation и checksum карты совпали с ранее
принятой сборкой на 1С 8.3.27.2325. Исходная собственная копия не изменилась.
Повторного запуска 1С в этой проверке не было: сравнивались закреплённые
входы и XML-поколение, уже принятое платформой.

Самостоятельный project CLI подключит этот план при создании проекта ESKA.
Существующий native XML template не изменён. Политика Git LFS для выбранных
source/base отключается явно; сравнения хранения с LFS в EF-09 остаются отдельным
измерением, а автоматическая поставка LFS не добавлена.
