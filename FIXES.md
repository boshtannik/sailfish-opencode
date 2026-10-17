# Фиксы после первого теста голоса (2026-09-18)

Записал со слов пользователя перед сном. Приступаем к реализации в следующей сессии.

## Итог: все пункты закрыты + ещё два (2026-09-18, вечер)

- **Прогресс скачивания, cancel, удаление, отметка «выбрана»** — в `GlobalSettingsPage.qml` (пикеры STT/TTS: % , ✓, тап=cancel/выбор/скачивание, долгий тап=удалить/отменить) + `voice.rs` (`voice_download_cancel`, `voice_delete`, защита от двойного скачивания).
- **PTT-позиция** — `ChatPage.qml` читает `appWindow.pttPosition`, микрофон слева/справа от send.
- **Динь-перезапуск** — пул из 4 `SoundEffect` с ротацией слотов в `harbour-opencode.qml`.

## Падение при запуске через launcher (корень найден и устранён)

Симптом: приложение запускалось **только напрямую** (`/usr/bin/harbour-opencode`), а из лаунчера падало (паника tokio «RefCell already borrowed», либо SIGSEGV/SIGTRAP в `opencode-worker`) — мгновенно, без логов. Прямой запуск и даже пересборка чистого HEAD страдали одинаково, т.е. код не виноват.

Причина: `X-Nemo-Application-Type=silica-qt5` в `.desktop` заставлял lipstick запускать приложение через **mapplauncherd/booster**. Бустер `fork()`-ом создаёт процесс от долгоживущего многопоточного демона, и дочерний процесс наследует «грязное» glibc-состояние (кэш стеков/TCB/TLS убитых при fork потоков). Rust-рантайм (std + tokio) на старте трогает TLS/аллокацию и спотыкается об это наследие. C++/Qt-аппы не поднимают рантайм с потоками в этот момент — поэтому у них проблемы нет.

Решение (чистое, без костылей): **убрать `X-Nemo-Application-Type` из `harbour-opencode.desktop`** — приложение запускается напрямую (как `fingerterm`, `simkit` и др.). Это штатный путь SFOS. Заодно из `main.rs` убраны временные обходы (re-exec, retry-цикл, нестандартный стек) и отладочные маркеры; архитектура прежняя: async-стек (tokio) в отдельном std-потоке, QML — в главном.

## Голос / модели
- [x] **Прогресс и результат скачивания не видны (отчёт 2026-09-19).** Диагноз по device-логу: тап «Скачать» РАБОТАЕТ (STT 59 МБ и TTS 63 МБ реально скачались), но `refresh_status()` в Rust вызывался только на старте/конце — прогресс заморожен на 0%, а мгновенная ошибка уходила в «error» между QML-опросами (300 мс) и выглядела как «ничего не произошло». Пользователь, не видя фидбека, тапал повторно (в логе «уже качается» ×5).
  - [x] `voice.rs`: периодический `refresh_status()` раз в ~250 мс во время скачивания → QML видит живой % и полоску прогресса.
  - [x] `voice.rs`: сообщения в чат при старте/успехе/ошибке скачивания («[голос] скачивание «…»…», «…скачана», «ошибка скачивания …»).
  - [x] `VoiceModelsPage.qml`: состояние «error» теперь видно — иконка «!», подпись «download failed»; тап по ошибке = повторная попытка.
- [ ] Проверить вживую: при скачивании видно ли полоску/процент и метку «downloading»→«✓», при сбое сети — «!».

## Уведомления / звук
- [ ] **Динь играет через раз / только после окончания предыдущего:** `SoundEffect` нельзя перезапустить до конца воспроизведения. Нужно: прерывание/перезапуск ранее играющего ding, либо очередь/параллельное воспроизведение.

## PTT
- [ ] **Настройка «PTT button position» не влияет на кнопку:** в ChatPage микрофон закреплён слева и не читает `pttPosition` (left/right).
  - [x] Кнопка стала читать `pttPosition` (микрофон слева/справа от send).
  - [ ] **БАГ (новый отчёт):** при `pttPosition=right` кнопка становится справа, НО поле ввода вылезает за кнопку «Отправить»; возврат на «left» проблему не чинит в текущей сессии. Причина по коду: `input.left` привязан к `mic.right`, а когда микрофон справа — `mic.right` стоит рядом с send, и левая граница инпута схлопывается/влезает под send. Фикс: `input.left = micRight ? parent.left : (mic.visible ? mic.right : parent.left)`, `input.right = busy ? stop.left : (micRight ? mic.left : send.left)`. Нужно пересобрать и проверить переключение left↔right вживую.

## VoiceModelsPage — строки списка не видны вообще (2026-09-19)
- [x] **БАГ: после рефакторинга в `VoiceModelsPage.qml` с Loader-обёрткой в delegate строки списка перестали отображаться.** Правдоподобная причина: `delegate: Item { height: loader.height }` + Loader — `ListItem` внутри Loader не даёт implicitHeight (0), все строки схлопываются в нулевую высоту.
- [x] **СЕКЦИЯ ПЕРЕПИСАНА (V2):** гибридный ListItem с `modelData.*` заменён на `ListModel` с явными ролями (isHeader/kind/title/name/sub/state/modelId/engine/downloaded) — капризов с неявными полями больше нет. Счётчик прогресса исключён из фингерпринта: список пересобирается только при смене состояние/выбор/фильтр/секция, а живой прогресс % и полоска — отдельными live-биндингами по `pattern modelId` (скролл не сбрасывается во время скачивания). Иконка-действие в фиксированном слоте `Theme.itemSizeMedium` — текст на неё не наезжает. Заголовки секций показывают «(done/total)».
- [ ] Проверить вживую: видны ли строки, нет ли «undefined (undefined/undefined)», не наезжает ли текст на иконку, работает ли сворачивание/скачивание/выбор/удаление.

## Глобальные настройки — наложение лейблов под «Voice models» (рецидив)
- [x] **Лейблы разнесены по отдельным строкам.** По просьбе пользователя каждая подсказка — отдельная строка через `\n`: «Recognition:\n…\nSynthesiser:\n…» и легенда «↓ download\n→ start using\n✓ selected\n✕ cancel\nlong press — remove» со `wrapMode: Text.Wrap`.
- [ ] Проверить вживую читаемость.

## Статическая проверка QML до компиляции (просьба пользователя)
- [ ] Добавить скрипт/шаг, который проверяет QML до сборки: корректные атрибуты у компонентов Sailfish.Silica, синтаксис. Кандидаты: `qmllint` (qtdeclarative-tools) / `qmlimportscanner` / прогон через `qmlscene` в SDK-контейнере, либо простой парсер баланса скобок + проверка неизвестных свойств. Проверить наличие `qmllint` в docker-образе mersdk и подключить к сборке (жёлтые предупреждения ≠ ошибки, но бросающиеся ошибки атрибутов ловим до деплоя).
## UX-батч чата: план и статус (для следующего ИИ, 2026-09-23)

> Работа идёт ПОЭТАПНО: правка → `./mbuild.sh` (через docker) → scp RPM на телефон → `devel-su rpm -Uvh --force` → чистка `~/.cache/harbour-opencode/qmlcache` → перезапуск через `invoker` → проверка вживую → коммит. Каждый этап — отдельный коммит. Текущий HEAD: `1eb390c` (этап 3 ч.1 — hit-область кнопки озвучки), этапы 3 ч.2–3.3 задеплоены (подтверждены пользователем), коммит ожидается вместе с обновлением этого файла.

### Сделано и закоммичено (этапы 1–2.5)
- [x] **Группировка ответа ИИ** — `acc.concat` собирает стрим-строки в один «говорящий» баббл.
- [x] **Ошибки сервера в ленту** — `human_session_error()` (`types/event.rs`), баббл + нотификация, без дублей (`continue` после `acc.concat`).
- [x] **Детали квоты/responseBody** — `APIError` 401/402/429 показывает `responseBody` («Resets at 14:32 UTC»).
- [x] **Непрозрачная карточка доступа** — permission-оверлей читается, не залезает за экран, затемнение фона отдельным Rectangle.
- [x] **Мок пермишнов через воркер** — `Cmd::MockPermission` → диспетчер → отложенный снимок не стирает карточку.
- [x] **Dev-панель** в ChatPage (`mockErrPanel`): 6 мок-ошибок + «Permission ask» + «Burst: 3 errors at once».
- [x] **Убран debug-спам** `DBG perms snapshot` из `refresh_permissions_json`.

### Осталось (переносить из stash частями — НЕ целиком!)
- [x] **Этап 3. TTS-кнопка с лоадером** — `speakBtn` в `ChatPage.qml:266` сейчас hit-область всего `Theme.iconSizeSmall` (~мелкая). Задача: hit-область ≥ `Theme.iconSizeMedium`, а во время синтеза/озвучки на месте иконки — лоадер (BusyIndicator) конкретно у этого баббла. Статус «синтезируется/озвучивается» — из `voice_status_json` (`bridge/app.rs`, голосовой статус пишется из `voice.rs`).
  - [x] **ч.1** hit-область `Theme.itemSizeMedium` + футер баббла шире (коммит `1eb390c`).
  - [x] **ч.2** Лоадер на кнопке именно этого баббла: `harbour-opencode.qml` — свойства `speakingBubble`/`ttsSynth`, сброс в `playNextTts`/`stopTts`, установка в `speakText`; `ChatPage.qml` — `BusyIndicator` на `speakBtn` при `(ttsSynth || ttsPlaying) && speakingBubble === line.body`.
  - [x] **ч.3** Стоп-в-баббле вместо отдельной кнопки `ttsStopBtn` (удалена): кнопка озвучки активного баббла превращается в «стоп» (`icon-m-stop` + `highlighted`), клик = `stopTts()`. Честная отмена синтеза: `Cmd::VoiceTtsCancel` (`cmd.rs`), `cancel_tts: Arc<Mutex<bool>>` в `VoiceState`, проверка в `run_tts` (не отдаёт WAV + удаляет файл), сброс флага в `tts_from_call`; из QML `stopTts()` шлёт `voiceCmd(cmdTtsCancel)`.
  - [x] **ч.4** Исчерпывающие match'и: `main.rs` диспетчер (был без catch-all), в `voice.rs::run_command` заглушка `_ => {}` заменена на явный перечень не-голосовых `Cmd::*` (+ ветка `None`) — расширение `Cmd` теперь требует правки и диспетчера, и голосового обработчика.
- [x] **Этап 4. Markdown-рендер** — `mdEscape/mdInline/mdBlock` в `ChatPage.qml` + `Text.RichText` + `onLinkActivated` (коммиты `1ca5275` + `…`): заголовки h1–h3 с градацией размера (`<font size>`), жирный, курсив `*…*`/`_…_`, обычный и нумерованный списки (номер сохраняется), fenced-блоки через `<pre>` (переносы и отступы кода дословно), инлайн-код, ссылки в т.ч. с title, цитаты, `---`. Ссылки кликабельны (`Qt.openUrlExternally`). Подтверждено на телефоне пользователем.
- [ ] **Этап 5. Контекстное меню Copy code/Copy message** — буфер через мост/`QClipboard` (НЕ глобальный `Clipboard` из Silica — белый экран), hit-область 40px+, переводы.

### Грабли (помнить!)
- Глобальный `Clipboard` (Sailfish.Silica `Clipboard`) **недоступен** — pure-maps объявляет его в СВОЁМ платформенном слое; в нашем `pure` его нет → белый экран. Копирование — только через мост/`QClipboard`.
- Перед перезапуском приложения на телефоне: `pkill -f "opencode serve"` (ручной serve держит SQLite-лок БД → приложение поднимает свой serve и тот умирает `exit status 1`).
- После деплоя всегда чистить `rm -rf ~/.cache/harbour-opencode/qmlcache`, иначе QML старый.

## Дубль живого ответа в ленте (2026-10-15) — найден корень, исправлено

**Симптом (со слов пользователя):** в живой ленте один и тот же ответ ИИ иногда виден ДВАЖДЫ; кнопка «стоп» TTS подсвечивается на обоих бабблах. После сворачивания и повторного входа в чат (перечитка истории с сервера) дубли исчезают — значит баг в ЖИВОМ пути отрисовки, а не в истории/модели.

**Корень (точно, по логам воркера + QML):** живой ответ стримится дельтами `[[answerlive]]` (воркер шлёт накопленный текст, троттлинг 150 мс). Затем приходит строка инструмента `[[tool]] call_… running` — и уже ПОСЛЕ неё воркер шлёт финальный `answerlive-final` (текст-end, `message.part.updated`). Так устроен поток событий сервера: `part.updated` инструмента приходит раньше финального `part.updated` текста. А замена в QML была **позиционной** — смотрела ТОЛЬКО на последнюю строку `acc[acc.length-1]`. Хвост к этому моменту уже tool-строка, поэтому финальный пуш не заменял живой баббл, а ДОБАВЛЯЛ второй с тем же текстом.

Лог-подтверждение (один шаг, воркер → QML):
```
push answerlive ... len=53
push tool callID=call_b45f... running   <-- вклинилось между живым и финалом
push answerlive-final ... len=53        <-- хвост уже tool => QML APPEND
ALFED append len=53 prev=[[tool]]call_b45f...   <-- дубль
```
Тот же паттерн повторялся в КАЖДОМ шаге, где модель пишет текст перед вызовом инструмента.

**Разбор перед этим:** сначала показалось, что это утечка токена `</think>` от модели — НЕТ. То отдельный артефакт модели (правило AGENTS.md #7, не чиним). Дубль ответа — наш баг живого фида (правило применили, классифицировали origin).

**Фикс:** заменить позиционную эвристику на замену ПО `partID`, позиционно-независимую.
- Протокол: живые строки несут id парта в хвосте — `[[answerlive]]<текст>[[p:<partID>]]` (и `[[thinklive]]…[[p:<partID>]]`). Новый маркер `PART_ID_OPEN = "[[p:"` (`markers.rs`), билдер `proto_live_line(ts, tag, text, part_id)`.
- Воркер (`event/dispatcher.rs` живые дельты, `main.rs` финалы answerlive-final/thinklive-final) шлёт id; финал и живые дельты ОДНОГО парта несут один id → финал редактирует тот же баббл.
- QML: хелперы `partIdOf` / `stripPartId` / `liveIndexOf` прямо в `harbour-opencode.qml` (поиск баббла по id сканом с хвоста). `poll()` для `[[answerlive]]`/`[[thinklive]]` заменяет баббл по id, если найден, иначе добавляет. Делегат (`ChatPage.qml`) срезает `[[p:…]]` в `body`; титул/озвучка (`lastAnswer`) — тоже.
- Так финал приходит ПОСЛЕ tool-строки — но всё равно находит свой баббл по id и правит его на месте. Дубль невозможен, даже если между дельтой и финалом вклинилось сколько угодно строк.

**Тесты (задел на будущее).** Логика протокола вынесена в отдельный крейт без зависимостей — `opencode-proto/` (`markers.rs` переехал туда, `opencode-client/src/markers.rs` — тонкий ре-экспорт), поэтому тесты гоняются на хосте обычным `cargo test`, без SFOS-тулчейна:
- `opencode-proto/src/lib.rs` — формат строк (`proto_live_line` несёт `<partID>` в хвосте, `proto_user_line`, `proto_tool_line`), т.е. инвариант, на котором стоит фикс. Прогон: `make test`.
- `opencode-client/src/event/dispatcher.rs` — `#[cfg(test)] mod tests`: живая дельта проходит путь `on_message_part_delta` и пушит строку с id парта (answer/think), а дельта не-assistant сообщения игнорируется. Крейт линкует SFOS/Qt, поэтому гоняется в SDK через sb2+qemu: `make test-sdk`.

## Компакция сессии — нет обратной связи (2026-10-16)

**Симптом (со слов пользователя):** нажал «Compact (compress history)» в настройках сессии — экран переключается на чат, и до появления готового баббла (выжимки) на экране ничего не происходит: непонятно, идёт работа или нет. Результат виден только если выйти и заново войти в чат.

**Диагноз (по логам телефона + живой записи SSE + коду сервера):**
- Классический HTTP-API сервера шлёт ТОЛЬКО: user-сообщение с партом `type:"compaction"`, затем assistant-сообщение `summary:true` (текст выжимки), и в конце `session.compacted`. Событий `session.next.compaction.started/delta/ended` НЕТ.
- Замер: `POST /session/{id}/summarize` **блокирует ~20 с** и возвращает ответ только когда компакция уже завершена. Воркер (`run_stream`) ждёт этот POST прямо в ветке обработки команд `tokio::select!`, поэтому события SSE (compaction-парт, `session.compacted`) копятся в буфере и обрабатываются одним залпом уже ПОСЛЕ завершения. Плашка «Compacting…» ставится и тут же стирается `[[clearfeed]]`; QML (опрос раз в 300 мс) её вообще не успевает увидеть. Отсюда «ничего не происходит ~20 с».
- `Cmd::Summarize` (`main.rs`) делал только POST + `log::info!` (логгер на `warn`, не виден): ни статуса, ни строки в ленте. Загрузчик истории молча пропускал парт `compaction`.

**Фикс:**
- **Флаг «идёт компакция» в bridge** (`compacting_shared: Arc<Mutex<bool>>`, `compaction_active()` для QML). Ставится **мгновенно** в `Cmd::Summarize` ДО блокирующего POST (для текущей сессии) и в диспетчере на событие compaction-парта (это же покрывает АВТО-компакцию при переполнении контекста). Сбрасывается на `session.compacted` и на `session.idle`.
- **Плашка активности** над чатом (`ChatPage.qml`): `BusyIndicator` + локализованный текст «Compacting history…» / «Сжимаю историю…» (`kCompactingNotice` в `harbour-opencode.qml`, чтобы `qsTr` резолвился в нужном контексте перевода). Пока флаг поднят — плашка видна; список чата сдвигается вниз на её высоту.
- **`session.compacted` → перезагрузка истории:** диспетчер вынесен из «ignored» в явную ветку — пишет id сессии в `StreamState::compacted` (IO остаётся в SSE-цикле). Цикл (`main.rs`) для текущей сессии сбрасывает стрим-аккумуляторы, шлёт control-строку `[[clearfeed]]` и перезагружает историю + сессии + todo. Так выжимка и маркер появляются сразу, без перезахода.
- **Маркер в истории:** загрузчик рендерит парт `compaction` как сервис-строку `svc_line(created, K_COMPACTED)` («Context compacted» / «Контекст сжат»); пустое user-сообщение с этим партом больше не даёт пустой баббл.
- **Протокол:** новый маркер `CLEAR_FEED_TAG = "[[clearfeed]]"` (`opencode-proto/src/lib.rs`, зеркало `kClearFeedTag` в QML) — QML по нему очищает ленту перед вставкой перезагруженной истории.
- i18n: ключи `K_COMPACTING` / `K_COMPACTED` (`i18n.rs`) + во всех `.ts` (ru/uk/de/fi переведены).

**Тесты:** `opencode-client/src/event/dispatcher.rs` — `#[tokio::test] session_compacted_records_the_session_to_reload` (диспетч `session.compacted` пишет id в `StreamState::compacted`) и `compaction_part_announces_once_for_the_shown_session` (одна плашка на парт; взвод флага на текущую сессию, игнор чужой; сброс на `session.compacted`). Прогон: `make test` (proto, хостер) + `make test-sdk` (5 lib-тестов, зелёные).

**Проверено вживую:** пользователь подтвердил плашку при ручном «Compact» (после релизной сборки).

## Тяжёлый payload → /home, GUI-лаунчер остаётся в /usr/bin (2026-10-17)

**Задача:** бандл (opencode CLI 185 МБ + piper + whisper-cli) занимал ~580 МБ на корневом разделе `/` (4.7 ГБ; свободно было ~262 МБ). `/home` — отдельный LUKS-раздел на 97 ГБ.

**Раскладка после изменения** (`rpm/harbour-opencode.spec`, `%define payloaddir /home/%{name}`):
- Тяжёлые хелперы → `/home/harbour-opencode/{opencode,whisper-cli,piper/}`.
- `/usr/libexec/harbour-opencode` — **симлинк** на `/home/harbour-opencode`. Клиент обращается к хелперам абсолютными путями (`voice.rs`, `server.rs`), они резолвятся через симлинк.
- `/usr/share/harbour-opencode/{qml,translations,models.json}` — как раньше.
- `/usr/bin/harbour-opencode` — **реальный файл** (GUI-лаунчер), НЕ симлинк.

**ПОЧЕМУ лаунчер обязан быть реальным файлом (главная грабля):**
`libsailfishapp` вычисляет каталог данных из каталога исполняемого файла (`SailfishAppPriv::dataDir()`, `sailfishapp_priv.cpp`): берёт `exe.absolutePath()`, заменяет последний компонент пути на `share/` и приклеивает имя приложения. Если `/usr/bin/harbour-opencode` — симлинк, `/proc/self/exe` резолвится в `/home/harbour-opencode/...`, и dataDir становится `/home/share/harbour-opencode` → главный QML (`QmlApp::path_to`) не найден → **белый экран** (без внятной ошибки в логе). `applicationPath()` берёт абсолютный argv[0] как есть, иначе `applicationFilePath()`; лаунчер передаёт bare `Exec=harbour-opencode` (в `.desktop`), поэтому симлинк всё ломает.
Симптом на глаз: «открываю приложение — белый экран» (не путать с ошибкой QML/синтаксиса — здесь QML вообще не грузится).
Быстрая диагностика: структура RPM — `7z x -so harbour-opencode-*.rpm | cpio -itv`; exec через симлинк — `ls -l /usr/libexec/harbour-opencode/opencode`.

**Освобождение корня:** на `/` остались только GUI-бинарник (~18 МБ) + QML/иконки/переводы; `df -h /`: с ~262 МБ до ~786 МБ свободно.

**Грабли обновления (каталог→симлинк):**
- Прежняя раскладка держала `/usr/libexec/%{name}` как КАТАЛОГ; новая — СИМЛИНК. In-place `rpm -Uvh` падает: `cpio: File from package already exists as a directory in system`. А `rpm -e` иногда оставляет ПУСТЫЕ каталоги → установка снова падает.
- Рабочая процедура перехода (разовая):
  1. `pkill -x harbour-opencode` (и `harbour-glas`).
  2. `devel-su rpm -e --nodeps harbour-opencode harbour-glas`.
  3. `devel-su rm -rf /usr/libexec/harbour-opencode /usr/libexec/harbour-glas /usr/share/harbour-opencode /usr/share/harbour-glas /home/harbour-opencode /home/harbour-glas /usr/bin/harbour-opencode /usr/bin/harbour-glas` (+ `.desktop`/иконки при надобности).
  4. `devel-su rpm -Uvh harbour-opencode-*.rpm harbour-glas-*.rpm`.
- Последующие обновления (симлинк→симлинк) обычным `rpm -Uvh` проходят нормально — чистка нужна только на разовый переход.

**rpmlint (ожидаемо, сборку не роняет):** `E: dir-or-file-in-home` (файлы в /home) и `E: binary-or-shlib-defines-rpath … $ORIGIN` (piper — пришёл из готового aarch64-билда). В сборочном образе включён `TreatErrorsAsWarnings`, поэтому успех.

**mbuild.sh (грабля кавычек):** docker-скрипт целиком в ОДИНАРНЫХ кавычках (чтобы `$VAR` доехали до контейнера), поэтому внутренние кавычки надо писать ДВОЙНЫМИ. Иначе `printf '%s\n'` доезжало как `printf %sn` и в `opencode.version` попадало `1.19.0n` → opencode качался заново на каждой сборке. Verify теперь извлекает `./home/harbour-opencode/opencode`.

**Затронуто:** `rpm/harbour-opencode.spec`, `mbuild.sh`; в репозитории Glas — `rpm/harbour-glas.spec` (то же решение).

## Сервер не стартовал: пустой workdir в settings.json (2026-10-17)

**Симптом:** после установки RPM с переносом payload в `/home` при открытии приложения GUI не мог подключиться к `opencode serve`. В `app.log` одна строка:
`failed to start opencode serve: не удалось запустить /usr/libexec/harbour-opencode/opencode: Нет такого файла или каталога (os error 2)`.

**К муву в /home баг отношения НЕ имел** — совпадение по времени. Бинарник через SSH запускался (`opencode --version` → 1.19.0), симлинк `/usr/libexec/harbour-opencode` → `/home/harbour-opencode` на месте.

**Цепочка бага:**
1. Страница настроек (`GlobalSettingsPage.qml`): `onActiveFocusChanged` у поля workdir сохраняет `text` даже ПУСТЫМ → `set_workdir("")` → `settings.json` = `{"workdir":""}`.
2. При старте `Settings::load()`: `#[serde(default = "default_workdir")]` срабатывает только при ОТСУТСТВИЮЩЕМ ключе; пустая строка проходила как есть → `workdir=""`.
3. `cmd.current_dir("")` → в дочернем процессе `chdir("")` → ENOENT; Rust `Command::spawn` возвращает этот errno как «os error 2» (создание каталога молча игнорировалось: `let _ = create_dir_all`).

**Фикс (defense in depth):**
- `settings.rs`: `Settings::sanitize()` — пустой/whitespace workdir заменяется дефолтом в `load()` и `save()` (лог `workdir is empty — falling back to the default`);
- `server.rs`: `ServerGuard::start` отфильтровывает пустой `cwd` перед `current_dir` + громкий лог при неудачном `create_dir_all`;
- `main.rs`: `create_dir_all` рабочего каталога больше не игнорируется молча.

**Диагностика на телефоне:** `cat ~/.config/harbour-opencode/settings.json` (пустой workdir = вот он), `tail ~/.cache/harbour-opencode/app.log`, `ls -l /usr/libexec/harbour-opencode/opencode`, ручной запуск бинарника по тому же пути.

**Затронуто:** `opencode-client/src/settings.rs`, `opencode-client/src/server.rs`, `opencode-client/src/main.rs`.

## Сжатие истории не росло живьём + залипал баннер (2026-10-18)

**Симптом:** при компакции (кнопка Compact или авто-сжатие по переполнению) саммари появлялось в чате только после пере-открытия сессии; во время работы — лишь плашка «Compacting…». Если переключиться в другую сессию во время сжатия, плашка могла остаться.

**Как это делает сам opencode (v1.19.0, сверялись с исходниками `packages/opencode/src/session/compaction.ts`):** саммари — это ОБЫЧНОЕ assistant-сообщение с `mode:"compaction", summary:true`, которое стримится штатными `message.part.updated`/`message.part.delta`. TUI рисует его как рядовой бабл, растущий вживую, а сверху — разделитель « Compaction ». События `session.next.compaction.*` в 1.19.0 не публикуются вовсе (в HEAD/2.x имена другие — `session.compaction.delta`).

**Цепочка багов у нас:**
1. Диспетчер намеренно игнорировал `mode:"compaction"` (`dispatcher.rs`): сообщение не попадало в `assistant_messages`, его дельты отбрасывались → бабл не рос.
2. Перезагрузка истории по `session.compacted` выполнялась только если сжимаемая сессия = текущая; иначе событие «съедалось» (`compacted_session.take()`), и саммари появлялось лишь при возврате в сессию.
3. `compacting`/`compaction_seen` не сбрасывались при переключении сессии (`Cmd::Open`) → «Compacting…» могло залипнуть.

**Фикс:**
- `dispatcher.rs`: compaction-сообщение регистрируется в `assistant_messages` (саммари стримится как обычный ответ) и дополнительно в `compaction_messages`.
- `main.rs`: `compaction_messages` исключает саммари из авто-TTS (обе ветки: text-end и idle-fallback) — плашка/бабл видны, но текст не читается вслух.
- `main.rs`: `Cmd::Open`/`New`/`Fork`/`DeleteAll` сбрасывают `compaction_seen` и `compacting`; ветка `session.compacted` сбрасывает `compaction_seen` для любой сессии (reload — только для текущей, с логом `…compacted in background`).
- `types/keys.rs`: `MODE_COMPACTION` — чтобы не было магической строки в сравнении.

**Затронуто:** `opencode-client/src/event/dispatcher.rs`, `opencode-client/src/main.rs`, `opencode-client/src/types/keys.rs`.
