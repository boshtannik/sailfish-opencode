import QtQuick 2.6
import Sailfish.Silica 1.0
import QtMultimedia 5.4
import Nemo.Notifications 1.0
import Nemo.KeepAlive 1.2
import Nemo.Configuration 1.0
import "pages"

ApplicationWindow {
    id: app

    // Custom cover (обложка): the last agent answer + two quick actions
    // (CoverPage.qml). Defined inline below so it closes over `app`.
    cover: coverComponent

    // Playback is NOT cut when the app is minimized: TTS keeps speaking in the
    // background (KeepAlive below keeps the process up while it speaks) and the
    // cover's second action turns into "stop" for as long as TTS is active.
    // NOTE: `onClosing` is NOT usable on Silica's ApplicationWindow (private
    // Window) — assigning it yields a blank window (the old RAII "silence on
    // leave" via applicationActive is gone on purpose: it swapped the app for
    // the cover and killed the speech mid-answer).

    // Source of truth for the feed (plain JS array; poll() builds it with
    // concat/slice). The ListView does NOT use it directly — it is mirrored
    // into `chatModel` incrementally (see syncModel) so the view never resets
    // its scroll position. Replacing the whole array as the ListView model
    // (e.g. on every streamed delta) destroyed all delegates and jumped to top.
    property var messages: []
    // True while the shown session is being compacted (server-driven; read
    // from the bridge each poll). Drives the activity banner above the chat.
    property bool compactionActive: false
    ListModel { id: chatModel }
    property alias chatModel: chatModel
    onMessagesChanged: app.syncModel(app.messages)

    // Mirror the plain `messages` array into the ListModel incrementally:
    // update changed items in place (setProperty), append the tail, drop the
    // excess. The ListView then keeps its scroll position — unlike swapping the
    // whole array model, which resets to the top on every update.
    function syncModel(next) {
        var n = next.length
        while (chatModel.count > n)
            chatModel.remove(chatModel.count - 1)
        for (var i = 0; i < n; i++) {
            if (i < chatModel.count) {
                if (chatModel.get(i).text !== next[i])
                    chatModel.setProperty(i, "text", next[i])
            } else {
                chatModel.append({ "text": next[i] })
            }
        }
    }
    property string statusText: app.kServerStatusConnecting
    property string inputMode: inputModeSetting.value !== undefined
                               ? inputModeSetting.value : "text_ptt"
    property var sessions: []
    property var todos: []
    property var models: []
    property string currentSessionId: ""
    // True while the session tail is reverted (worker control line [[reverted]]1):
    // shows the "Restore reverted messages" banner above the input.
    property bool reverted: false
    // How tool calls are shown in the transcript. Mirrors the official TUI
    // (Settings → Session → Tool calls): "show" (name + argument), "minimal"
    // (name only) or "hide" (no tool rows). Stored globally, like the TUI key.
    property string toolCalls: toolCallsSetting.value !== undefined
                               ? toolCallsSetting.value : "show"
    // True only in a debug build: enables the mock buttons (errors/permissions).
    readonly property bool devTools: bridge !== undefined
                                     && bridge.dev_tools()
    // Position of the dictation (PTT) button. The default depends on the mode:
    // in voice-only it is the center (most natural), in text+PTT — left.
    property string pttPosition: pttPositionSetting.value !== undefined
                                 ? pttPositionSetting.value
                                 : (app.inputMode === "voice" ? "center" : "left")
    property string ttsMode: ttsModeSetting.value !== undefined
                             ? ttsModeSetting.value : "button"
    property bool soundOnFinish: soundOnFinishSetting.value !== undefined
                                 ? soundOnFinishSetting.value : true
    property bool soundOnPermission: soundOnPermissionSetting.value !== undefined
                                        ? soundOnPermissionSetting.value : true
    property Component sessionNameDialog: sessionNameDialogComponent
    property Component sessionNameDialogComp: sessionNameDialogComponent
    property Component sessionSettingsPageComponent: sessionSettingsPageComponent
    property bool sendImmediately: sendImmediatelySetting.value !== undefined
                                   ? sendImmediatelySetting.value : false
    // OpenCode Console device-login state (worker → QML): {state,url,code,message}.
    property var consoleLogin: ({ "state": "idle", "url": "", "code": "", "message": "" })
    // Verification URL already opened in the browser (auto-open only once).
    property string consoleLoginOpenedUrl: ""
    // ── Worker ↔ QML protocol markers ─────────────────────────────────────────
    // Mirror of opencode-client/src/markers.rs. Change both files in sync:
    // conditions compare via these constants, never via raw strings — this
    // removes a whole class of bugs (there used to be substring(0,8) against
    // the 9-char [[think]]).
    readonly property string kTsOpen: "[[t:"
    readonly property string kTsClose: "]]"
    readonly property string kThinkTag: "[[think]]"
    // Live reasoning chunk (`[[thinklive]]<text>`): the worker streams model
    // reasoning deltas; QML replaces the trailing [[think]] bubble in place.
    readonly property string kThinkLiveTag: "[[thinklive]]"
    // Live answer chunk (`[[answerlive]]<text>`): the worker streams deltas of
    // every answer text part; QML grows/replaces the trailing answer bubble in
    // place so the answer is visible while the model types.
    readonly property string kAnswerLiveTag: "[[answerlive]]";
    readonly property string kUserPrefix: ">>> "
    readonly property string kSttTag: "[[stt]]"
    readonly property string kTtsTag: "[[tts]]"
    readonly property string kTtsStopTag: "[[ttsstop]]"
    readonly property string kSttDoneTag: "[[sttdone]]"
    // Revert/fork plumbing (mirror of markers.rs): `kMsgIdOpen` closes the
    // message-id marker appended to a user-history line `…[[m:<id>]]`;
    // `kSetMsgIdTag` is a control line `[[msgid]]<id>` attaching the id of a
    // just-sent user message to the last id-less user bubble; `kRevertedTag`
    // is a control line `[[reverted]]<0|1>` carrying the session revert flag.
    readonly property string kMsgIdOpen: "[[m:"
    readonly property string kSetMsgIdTag: "[[msgid]]"
    readonly property string kRevertedTag: "[[reverted]]"
    // Control line `[[clearfeed]]`: drop the whole visible transcript. The
    // worker emits it just before re-loading a session's history after the
    // server reported the session was compacted, so the reloaded history
    // replaces the feed instead of being appended to it.
    readonly property string kClearFeedTag: "[[clearfeed]]"
    // Live-line part-id marker (mirror of markers.rs PART_ID_OPEN): a live
    // answer/reasoning line ends with `[[p:<partID>]]`. QML edits the bubble
    // carrying that id in place — wherever it sits in the feed (see
    // `partIdOf`/`liveIndexOf`), so an interleaved tool row can no longer
    // duplicate the final answer.
    readonly property string kPartIdOpen: "[[p:"
    // Inline tool call (mirror of markers.rs): one feed line per tool call,
    // `[[tool]]<callID>\u001f<name>\u001f<status>\u001f<text>`, replaced in
    // place as the call moves running → completed/error (TUI-style).
    readonly property string kToolTag: "[[tool]]"
    // Server daemon connection statuses (mirror of markers.rs STATUS_*).
    readonly property string kServerStatusConnecting: "connecting"
    readonly property string kServerStatusIdle: "idle"
    readonly property string kServerStatusBusy: "busy"
    readonly property string kServerStatusError: "error"
    readonly property string kServerStatusRetryPrefix: "retry|"
    // Tool statuses (mirror of types/tool.rs STATUS_*).
    readonly property string kStatusRunning: "running"
    readonly property string kStatusCompleted: "completed"
    readonly property string kStatusError: "error"
    // i18n tokens (worker sends a translation KEY; QML resolves qsTr): see
    // opencode-client/src/i18n.rs. `[[s:KEY]]` — service notice,
    // `[[e:KEY]]` — error (red bubble + notification), args after \u001f.
    readonly property string kSvcOpen: "[[s:"
    readonly property string kErrOpen: "[[e:"
    readonly property string kTagClose: "]]"
    readonly property string kArgSep: "\u001f"
    // Bubble markers (built by QML for system styling; the worker itself
    // never sends them). kErrTag — error (red), kSvcTag — service notice.
    readonly property string kErrTag: "[[e]]"
    readonly property string kSvcTag: "[[s]]"
    // Compact banner label. qsTr is resolved HERE (context "harbour-opencode",
    // where the translation lives); ChatPage only reads the ready string.
    readonly property string kCompactingNotice: qsTr("Compacting history…")
    // Safety cap for a single feed bubble (display): a bloated single answer
    // must not wedge the main thread's rich-text layout on every feed rebuild.
    readonly property int kFeedBodyCap: 60000
    // Escape regex special characters (built from the marker constants).
    function reEscape(s) {
        return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
    }
    // Timestamp regexp `[[t:<ms>]]`, built from the constants (markers.rs:
    // proto_line). Group 1 — milliseconds. Conditions/slices go through the
    // constants, never raw strings.
    readonly property var tsRe: new RegExp(
        "^" + reEscape(app.kTsOpen) + "(\\d+)" + reEscape(app.kTsClose))
    // Strip `[[t:<ms>]]` from the start of a feed line.
    function stripTs(s) {
        return s.replace(app.tsRe, "")
    }
    // MessageID carried by a user line (`…[[m:<id>]]`), or "" when absent.
    function msgIdOf(line) {
        var b = app.stripTs(line)
        var i = b.indexOf(app.kMsgIdOpen)
        if (i < 0)
            return ""
        var j = b.indexOf(app.kTagClose, i + app.kMsgIdOpen.length)
        if (j < 0)
            return ""
        return b.substring(i + app.kMsgIdOpen.length, j)
    }
    // User-line body without the trailing `[[m:<id>]]` marker.
    function stripMsgId(b) {
        var i = b.indexOf(app.kMsgIdOpen)
        if (i < 0)
            return b
        var j = b.indexOf(app.kTagClose, i + app.kMsgIdOpen.length)
        if (j < 0)
            return b
        return b.substring(0, i) + b.substring(j + app.kTagClose.length)
    }
    // Feed line starts exactly with the marker-prefix (timestamp excluded).
    function lineIs(line, tag) {
        return line.indexOf(tag) === 0
    }
    // callID carried by an inline tool line (`[[tool]]<callID>\u001f…`), or "".
    function toolCallId(line) {
        var b = app.stripTs(line)
        if (!app.lineIs(b, app.kToolTag))
            return ""
        var rest = b.substring(app.kToolTag.length)
        var i = rest.indexOf(app.kArgSep)
        return i < 0 ? rest : rest.substring(0, i)
    }
    // Field `idx` of an inline tool line body (0=callID, 1=name, 2=status, 3=text).
    function toolField(line, idx) {
        var b = app.stripTs(line)
        if (!app.lineIs(b, app.kToolTag))
            return ""
        var parts = b.substring(app.kToolTag.length).split(app.kArgSep)
        return parts[idx] !== undefined ? parts[idx] : ""
    }
    // Part-id of a live answer/reasoning line (`…[[p:<id>]]`), or "".
    function partIdOf(line) {
        if (line === undefined || line === null)
            return ""
        var i = line.indexOf(app.kPartIdOpen)
        if (i < 0)
            return ""
        var j = line.indexOf(app.kTagClose, i + app.kPartIdOpen.length)
        if (j < 0)
            return ""
        return line.substring(i + app.kPartIdOpen.length, j)
    }
    // Display body without the trailing part-id marker.
    function stripPartId(b) {
        if (b === undefined || b === null)
            return ""
        var i = b.indexOf(app.kPartIdOpen)
        if (i < 0)
            return b
        var j = b.indexOf(app.kTagClose, i + app.kPartIdOpen.length)
        if (j < 0)
            return b
        return b.substring(0, i) + b.substring(j + app.kTagClose.length)
    }
    // Index of the feed line whose part-id equals `partId` (tail-first), or -1.
    // `partId === ""` never matches (id-less lines are history/plain, not live).
    // The live merge edits the bubble with this id IN PLACE — wherever it sits —
    // so a tool row landing between deltas and the text-end re-push can no
    // longer duplicate the final answer (the old positional heuristic did).
    function liveIndexOf(acc, partId) {
        if (partId === "" || acc === undefined || acc === null)
            return -1
        for (var i = acc.length - 1; i >= 0; i--) {
            if (app.partIdOf(acc[i]) === partId)
                return i
        }
        return -1
    }
    // Millisecond ts from `[[t:<ms>]]`, or current time if missing.
    function msOf(line) {
        var m = app.tsRe.exec(line)
        return m ? m[1] : String(Date.now())
    }
    // Translate a worker i18n token (`[[s:KEY\u001fARG…]]` / `[[e:KEY…]]`)
    // into a display string using the app's active language (qsTr/.ts).
    function trSvc(tag, token) {
        var end = token.indexOf(app.kTagClose, tag.length)
        var inner = token.substring(tag.length, end > 0 ? end : token.length)
        var parts = inner.split(app.kArgSep)
        var text = qsTr(parts[0])
        for (var i = 1; i < parts.length && i <= 9; i++)
            text = text.split("%" + i).join(parts[i])
        return text
    }
    // Whether to show the model's "thinking" ([[think]] lines) in the feed,
    // not just final answers. Enabled by default. This is a client DISPLAY
    // setting (unlike the session mode), so it lives in the global settings,
    // not in the settings of a particular session.
    property bool showReasoning: showReasoningSetting.value !== undefined
                                 ? showReasoningSetting.value : true
    // Per-session chat mode: `{ "<sessionID>": "build"|"plan" }`. The mode is a
    // property of each session (agent: build — active work, plan —
    // planning); the choice is stored per session and applied when it is
    // opened (see sessionMode/setSessionMode and its use in poll()).
    property var sessionModes: sessionModesSetting.value
                               ? JSON.parse(sessionModesSetting.value)
                               : ({})
    // Sessions whose saved mode has already been applied (we don't poke the
    // server again on every poll with the same agent switch).
    property var sessionModesApplied: ({})
    // Pending agent permission requests
    // (`[{id,sessionID,action,resources,message,save,source,options}]`).
    // The worker reconciles the list with the server, so it is the source of
    // truth: a request is shown while the server still waits for an answer.
    property var pendingPermissions: []
    // The last failed answer (the card stays on screen and shows the reason).
    property string permissionError: ""
    // Saved ("always allow") rules of the server
    // (`[{id,action,resource,projectID}]`).
    property var savedPermissions: []
    // The optional reason sent with the answer (the model sees it).
    property string permissionReason: ""
    // Granting "Allow once": true if the request is really new
    // (so the sound fires only for newly appeared ones).
    property var knownPermissionIds: []
    // Pending forms from the model (`[{id,sessionID,title,fields:[…]}]`).
    // The worker reconciles the list with the server, so it is the source of
    // truth: a form is shown while the server still waits for its answer.
    property var pendingForms: []
    // The id of the form shown right now (a new id = a new form arrived).
    property string currentFormId: ""
    property bool formOverlayVisible: false
    // The fields of the current form (kept apart from `pendingForms`: the poll
    // replaces the list every second, and that must not rebuild the form).
    property var formFieldsCache: []
    // The answers of the current form: `{fieldKey: value}` — the value is a
    // string, a number, a boolean or an array of strings (as the server wants).
    property var formAnswers: ({})
    // The own answers typed by the user: `{fieldKey: text}` (for the fields
    // that allow a custom value).
    property var formCustom: ({})
    // The last validation error (shown above the buttons).
    property string formError: ""
    property bool sttModelReady: false
    // opencode working directory (setting). Loaded from bridge.settings_json().
    property string workdir: ""
    property bool ttsModelReady: chosenTts !== ""

    // For detecting the "agent busy → free" transition.
    property bool wasBusy: false

    // System sounds for notifications (scanned by the worker, `[{name,path}]`).
    property var sounds: []
    // Selected notification sound (saved in DConf settings).
    property string dingSound: dingSetting.value

    // Voice module: status from the worker (`voice_status_json`).
    property var voiceModels: []
    property variant voiceLangOptions: [ { id: "auto", name: "Auto (detect)" } ]
    property string chosenStt: ""
    property string chosenTts: ""
    property string voiceLang: "auto"
    // Whether the microphone is recording (PTT toggle).
    property bool recording: false
    // Whether speech recognition is running after the recording stopped.
    property bool recognizing: false
    // Whether the answer is being spoken (to show the "stop" button).
    property bool ttsPlaying: false
    // Text of the bubble currently being synthesised/spoken (for the loader
    // on the corresponding TTS button). Empty string — we speak nothing.
    property string speakingBubble: ""
    // Whether speech synthesis is in progress: the command was sent to the
    // worker, the WAV is not in the queue yet (the quietest phase, where feedback matters).
    property bool ttsSynth: false
    // Path of the WAV currently loaded in ttsPlayer. The worker owns the queue
    // and sends one file at a time; when playback stops we ack this path back
    // (voice_tts_played) so the worker releases the next item.
    property string ttsCurrentPath: ""

    // Protocol constants of the voice module.
    // The commands mirror `voice::cmd` in src/voice.rs, the engines — `voice::engine`,
    // the phases — `voice::Phase` (as_str). They may only be changed on both sides.
    // Note: QML forbids property names starting with an uppercase letter — lowercase only.
    readonly property string cmdLang: "voice_lang"
    readonly property string cmdSelectStt: "voice_select_stt"
    readonly property string cmdSelectTts: "voice_select_tts"
    readonly property string cmdDownload: "voice_download"
    readonly property string cmdDownloadCancel: "voice_download_cancel"
    readonly property string cmdDelete: "voice_delete"
    readonly property string cmdRecordStart: "voice_record_start"
    readonly property string cmdRecordStop: "voice_record_stop"
    readonly property string cmdStt: "voice_stt"
    readonly property string cmdTts: "voice_tts"
    readonly property string cmdTtsMode: "voice_tts_mode"
    readonly property string cmdTtsCancel: "voice_tts_cancel"
    readonly property string cmdTtsPlayed: "voice_tts_played"
    readonly property string cmdCatalogUpdate: "voice_catalog_update"
    readonly property string engineStt: "stt_whisper"
    readonly property string engineTts: "tts_piper"
    readonly property string stateDownloading: "downloading"
    readonly property string stateError: "error"

    // Protocol constants of the permission reply — must equal the serde names
    // of `PermissionReplyKind` in src/types/event.rs (lowercase).
    readonly property string replyOnce: "once"
    readonly property string replyAlways: "always"
    readonly property string replyReject: "reject"

    // Request to switch the carousel page (0..3).
    signal requestPage(int index)

    // Recognised speech text that should be shown in the chat input field.
    signal dictated(string text)

    ConfigurationValue {
        id: dingSetting
        key: "/apps/harbour-opencode/dingSound"
    }

    ConfigurationValue {
        id: inputModeSetting
        key: "/apps/harbour-opencode/inputMode"
    }
    ConfigurationValue {
        id: toolCallsSetting
        key: "/apps/harbour-opencode/toolCalls"
    }
    ConfigurationValue {
        id: pttPositionSetting
        key: "/apps/harbour-opencode/pttPosition"
    }
    ConfigurationValue {
        id: ttsModeSetting
        key: "/apps/harbour-opencode/ttsMode"
    }
    ConfigurationValue {
        id: soundOnFinishSetting
        key: "/apps/harbour-opencode/soundOnFinish"
    }
    ConfigurationValue {
        id: soundOnPermissionSetting
        key: "/apps/harbour-opencode/soundOnPermission"
    }
    ConfigurationValue {
        id: sendImmediatelySetting
        key: "/apps/harbour-opencode/sendImmediately"
    }
    ConfigurationValue {
        id: showReasoningSetting
        key: "/apps/harbour-opencode/showReasoning"
    }
    // Per-session mode registry: JSON string `{"<sessionID>":"build"|"plan",...}`.
    ConfigurationValue {
        id: sessionModesSetting
        key: "/apps/harbour-opencode/sessionModes"
    }

    // Keep the process alive while the agent works OR speech is being
    // synthesised/played (otherwise Sailfish puts it to sleep in the background
    // and the answer being read out stops mid-sentence when minimized).
    KeepAlive {
        id: agentKeepAlive
        enabled: app.statusBase(app.statusText) === "busy"
                 || app.ttsPlaying || app.ttsSynth
    }

    // Pool of "dings": SoundEffect cannot be restarted while playing, so
// on each call we take the next free slot (rotation).
    property int dingSlot: 0
    SoundEffect { id: ding1; source: app.dingSound }
    SoundEffect { id: ding2; source: app.dingSound }
    SoundEffect { id: ding3; source: app.dingSound }
    SoundEffect { id: ding4; source: app.dingSound }

    Notification {
        id: notify
        appName: "opencode"
        urgency: Notification.Normal
        expireTimeout: 6000
    }

    // Transient "copied to clipboard" feedback. IMPORTANT: this Silica module
    // has NO `Toast`/`InfoBanner` types (both fail with "is not a type" and
    // cause a white screen) — so the banner is a plain Rectangle+Label built
    // from guaranteed-available primitives only.
    Rectangle {
        id: copyToast
        visible: false
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.paddingLarge
        radius: Theme.paddingMedium
        color: Qt.rgba(0.05, 0.05, 0.10, 0.92)
        width: copyToastTxt.implicitWidth + 2 * Theme.paddingMedium
        height: copyToastTxt.implicitHeight + Theme.paddingSmall
        Label {
            id: copyToastTxt
            anchors.centerIn: parent
            text: copyToast.bannerText
            color: Theme.highlightColor
            font.pixelSize: Theme.fontSizeSmall
        }
        property string bannerText: ""
        Timer {
            id: copyToastTimer
            interval: 1300
            repeat: false
            onTriggered: copyToast.visible = false
        }
    }

    // Copy `text` to the system clipboard and flash a confirmation banner.
    function copyText(text) {
        if (text === undefined || text === "")
            return
        try {
            if (typeof Clipboard !== "undefined") {
                Clipboard.text = text
            }
        } catch (e) {}
        copyToast.bannerText = qsTr("Copied")
        copyToast.visible = true
        copyToastTimer.restart()
    }

    Timer {
        id: pollTimer
        interval: 300
        running: true
        repeat: true
        onTriggered: app.poll()
    }

    // TTS player. The worker (Rust) is the single source of truth for the
    // speech queue, for the "is the user dictating" decision and for
    // cancellation; QML only plays the one file it is told to play and reports
    // back when it stopped. This removes the QML↔Rust race between the two
    // state machines (a cancelled part could otherwise "ripen" and start
    // playing after the user already stopped it).
    MediaPlayer {
        id: ttsPlayer
        onPlaybackStateChanged: {
            if (ttsPlayer.playbackState !== MediaPlayer.StoppedState)
                return
            // Only report a stop we did NOT initiate ourselves. Assigning a new
            // `source` also resets the state to Stopped while `ttsPlaying` is
            // still false, so that transient must not be acked.
            var donePath = app.ttsPlaying ? app.ttsCurrentPath : ""
            app.ttsCurrentPath = ""
            app.ttsPlaying = false
            app.ttsSynth = false
            app.speakingBubble = ""
            if (donePath !== "")
                app.voiceCmd(app.cmdTtsPlayed, donePath)
        }
    }
    // Play the single file the worker handed us. The worker already decided
    // this is allowed now (not dictating, nothing else playing).
    function playTts(path, body) {
        if (path === "" || path === undefined)
            return
        // Assign the source first (it may reset the player to StoppedState);
        // only then mark the file active and start playback, so the state
        // handler above never acks a half-set file.
        ttsPlayer.source = "file://" + path
        app.ttsCurrentPath = path
        // The worker echoes the exact bubble text it synthesised, so the
        // "stop/speak" button follows the bubble actually sounding.
        app.speakingBubble = (body === "" || body === undefined) ? "" : body
        app.ttsPlaying = true
        app.ttsSynth = false
        ttsPlayer.play()
    }
    // Silence the local player without telling the worker (used when the
    // worker itself ordered the stop, or because a stop command is already on
    // its way — no point reporting a finished playback it will ignore anyway).
    function stopTtsPlayer() {
        app.ttsCurrentPath = ""
        app.ttsPlaying = false
        app.ttsSynth = false
        app.speakingBubble = ""
        ttsPlayer.stop()
    }
    // User stop (bubble button / cover): silence now and ask the worker to drop
    // its whole queue and invalidate any synthesis still running.
    function stopTts() {
        app.voiceCmd(app.cmdTtsCancel, "")
        app.stopTtsPlayer()
    }
    function speakText(text) {
        app.speakingBubble = text
        app.ttsSynth = true
        app.voiceCmd(app.cmdTts, text)
    }

    // Rebuild the feed ONLY when something actually changed: a plain
    // `app.messages = acc` swaps the array and forces the ListView to
    // destroy/recreate every delegate (markdown→HTML + rich-text layout)
    // even for the same lines, on every 300 ms poll. Returns true if the
    // model array was replaced. `mutated` is set when a limb was edited
    // in place (live [[think]] bubble) — same array, but the feed MUST
    // refresh, so we hand the view a fresh copy.
    function applyFeed(acc, mutated) {
        if (mutated) {
            app.messages = acc.slice()
            return true
        }
        var cur = app.messages
        if (cur.length === acc.length) {
            var same = true
            for (var fi = 0; fi < acc.length; fi++)
                if (cur[fi] !== acc[fi]) {
                    same = false
                    break
                }
            if (same)
                return false
        }
        app.messages = acc
        return true
    }

    function poll() {
        bridge.gui_mark("start")
        var st = bridge.status_text()
        if (st !== undefined && st !== "")
            app.statusText = st
        app.compactionActive = bridge.compaction_active()
        if (app.statusBase(app.statusText) === "busy") {
            app.wasBusy = true
        } else if (app.wasBusy) {
            app.wasBusy = false
            app.notifyAgentFinished()
        }
        var msgs = bridge.drain_messages()
        if (msgs !== "" && msgs !== undefined) {
            bridge.gui_mark("feed-parse")
            var lines = msgs.split("\u001e")
            var acc = app.messages
            var accMutated = false
            for (var i = 0; i < lines.length; i++) {
                var line = lines[i]
                if (line !== "") {
                    // Line without the timestamp — used for [[think]] and for
                    // binding auto-TTS to the bubble.
                    var bodyTmp = app.stripTs(line)

                    // Server error (e.g. "free usage exceeded") — notify right
                    // away and push a red bubble, so it does not look like the
                    // agent is silently working. Worker sends `[[e:KEY]]`.
                    if (app.lineIs(bodyTmp, app.kErrOpen)) {
                        app.recognizing = false
                        var errText = app.trSvc(app.kErrOpen, bodyTmp)
                        app.publishError(errText)
                        acc = acc.concat(app.kTsOpen + app.msOf(line) + app.kTsClose
                                         + app.kErrTag + errText)
                        continue
                    }

                    // Service notice (voice/TTS/STT/catalog messages): worker
                    // sends `[[s:KEY\u001fARG…]]`, we translate by the app
                    // language and render as a SYSTEM bubble (no speak button,
                    // no auto-TTS binding — see isSvc in ChatPage).
                    if (app.lineIs(bodyTmp, app.kSvcOpen)) {
                        acc = acc.concat(app.kTsOpen + app.msOf(line) + app.kTsClose
                                         + app.kSvcTag + app.trSvc(app.kSvcOpen, bodyTmp))
                        continue
                    }

                    // Control line: the STT cycle finished (terminal message
                    // was sent before it). Stop the mic loader; not a bubble.
                    if (app.lineIs(bodyTmp, app.kSttDoneTag)) {
                        app.recognizing = false
                        continue
                    }

                    // Control line: the worker stopped the speech (the user
                    // pressed a stop button or started dictating). Silence the
                    // player; the worker already dropped its queue, so we must
                    // NOT report this stop back as a finished playback.
                    if (app.lineIs(bodyTmp, app.kTtsStopTag)) {
                        app.stopTtsPlayer()
                        continue
                    }

                    // Live model reasoning ([[thinklive]]): the worker streams
                    // deltas of `type:"reasoning"` parts while the model thinks.
                    // The line carries the part-id (`[[p:<id>]]`); we edit the
                    // bubble with that id IN PLACE — wherever it sits — so the text
                    // grows live instead of stacking a bubble per chunk, and a new
                    // block (think → tool → think) gets a fresh bubble (new id).
                    if (app.lineIs(bodyTmp, app.kThinkLiveTag)) {
                        if (!app.showReasoning)
                            continue
                        var rPart = app.partIdOf(bodyTmp)
                        var rText = app.stripPartId(bodyTmp.substring(app.kThinkLiveTag.length))
                        var rLine = app.kTsOpen + app.msOf(line) + app.kTsClose
                                    + app.kThinkTag + rText
                        if (rPart !== "")
                            rLine += app.kPartIdOpen + rPart + app.kTagClose
                        var rIdx = app.liveIndexOf(acc, rPart)
                        if (rIdx >= 0) {
                            acc[rIdx] = rLine
                            accMutated = true
                        } else {
                            acc = acc.concat(rLine)
                        }
                        continue
                    }

                    // Live answer ([[answerlive]]): the worker streams deltas of
                    // every answer text part while the model types. The line carries
                    // the part-id (`[[p:<id>]]`); we edit the bubble with that id IN
                    // PLACE — wherever it sits in the feed — so the answer grows live
                    // and the text-end re-push lands in the SAME bubble even if a
                    // tool row was appended in between (the old position-based
                    // replacement duplicated the answer in that case).
                    if (app.lineIs(bodyTmp, app.kAnswerLiveTag)) {
                        var aPart = app.partIdOf(bodyTmp)
                        var aText = app.stripPartId(bodyTmp.substring(app.kAnswerLiveTag.length))
                        var aLine = app.kTsOpen + app.msOf(line) + app.kTsClose + aText
                        if (aPart !== "")
                            aLine += app.kPartIdOpen + aPart + app.kTagClose
                        var aIdx = app.liveIndexOf(acc, aPart)
                        if (aIdx >= 0) {
                            acc[aIdx] = aLine
                            accMutated = true
                        } else {
                            acc = acc.concat(aLine)
                        }
                        continue
                    }

                    // Model reasoning ([[think]]) — Rust sends it as a line
                    // before the answer; hidden when the setting is off.
                    if (app.lineIs(bodyTmp, app.kThinkTag) && !app.showReasoning)
                        continue

                    // Terminal recognition result — drop the loader.
                    if (app.lineIs(line, app.kSttTag))
                        app.recognizing = false

                    // Recognised (STT): into the input or straight to the agent — depending on settings.
                    if (app.lineIs(line, app.kSttTag)) {
                        var text = line.substring(app.kSttTag.length).replace(/^\s+/, "")
                        if (text !== "") {
                            if (app.inputMode === "voice" || app.sendImmediately) {
                                // Bubble into acc: app.messages = acc comes after the loop,
                                // otherwise the bubble added by send() would be overwritten.
                                acc = acc.concat(app.kTsOpen + Date.now() + app.kTsClose
                                                + app.kUserPrefix + text)
                                bridge.send_prompt(text)
                            } else {
                                app.dictated(text)
                            }
                        }
                        continue
                    }

                    // TTS synthesis done: play the file the worker handed us.
                    // Format: `[[tts]]<path>\u001f<body>` — the body is the exact
                    // answer text the file belongs to (bound to its bubble).
                    if (app.lineIs(line, app.kTtsTag)) {
                        var ttsRest = line.substring(app.kTtsTag.length).replace(/^\s+/, "")
                        var ttsSep = ttsRest.indexOf(app.kArgSep)
                        var ttsPath = ttsSep >= 0 ? ttsRest.substring(0, ttsSep) : ttsRest
                        var ttsBody = ttsSep >= 0 ? ttsRest.substring(ttsSep + 1) : ""
                        if (ttsPath !== "")
                            app.playTts(ttsPath, ttsBody)
                        continue
                    }

                    // Control line: the id of a just-sent user message
                    // (`[[msgid]]<id>`). Attach it to the last user bubble that
                    // still lacks an id, so revert/fork work on live messages
                    // too (not only those restored from history).
                    if (app.lineIs(bodyTmp, app.kSetMsgIdTag)) {
                        var uid = bodyTmp.substring(app.kSetMsgIdTag.length).replace(/\s+$/, "")
                        if (uid !== "") {
                            for (var uk = acc.length - 1; uk >= 0; uk--) {
                                var ub = app.stripTs(acc[uk])
                                if (app.lineIs(ub, app.kUserPrefix)
                                        && app.msgIdOf(acc[uk]) === "") {
                                    acc[uk] = acc[uk] + app.kMsgIdOpen + uid + app.kTagClose
                                    accMutated = true
                                    break
                                }
                            }
                        }
                        continue
                    }

                    // Control line: the session revert flag (`[[reverted]]<0|1>`).
                    // Drives the "Restore reverted messages" banner.
                    if (app.lineIs(bodyTmp, app.kRevertedTag)) {
                        app.reverted =
                            bodyTmp.substring(app.kRevertedTag.length).replace(/\s+$/, "") === "1"
                        continue
                    }

                    // Control line: drop the transcript (`[[clearfeed]]`). The
                    // worker emits it before re-loading history after a
                    // compaction, so the reloaded feed starts from scratch.
                    if (app.lineIs(bodyTmp, app.kClearFeedTag)) {
                        acc = []
                        accMutated = true
                        continue
                    }

                    // Inline tool call ([[tool]]): one row per callID, replaced
                    // in place as the call moves running → completed/error
                    // (TUI-style inline tools in the transcript).
                    if (app.lineIs(bodyTmp, app.kToolTag)) {
                        if (app.toolCalls === "hide")
                            continue
                        var toolId = app.toolCallId(line)
                        if (toolId !== "") {
                            var tIdx = -1
                            for (var ti = 0; ti < acc.length; ti++) {
                                if (app.toolCallId(acc[ti]) === toolId) {
                                    tIdx = ti
                                    break
                                }
                            }
                            if (tIdx >= 0) {
                                // Keep the original position/timestamp; refresh
                                // only the payload (status/text).
                                acc[tIdx] = app.kTsOpen + app.msOf(acc[tIdx])
                                            + app.kTsClose + bodyTmp
                                accMutated = true
                            } else {
                                acc = acc.concat(line)
                            }
                        }
                        continue
                    }

                    acc = acc.concat(line)
                }
            }
            // Only re-push the model when it actually changed; the accMax debug
            // scan was removed (it walked the whole feed on every poll).
            app.applyFeed(acc, accMutated)
        }
        bridge.gui_mark("sessions")
        var sess = bridge.sessions_json()
        if (sess !== "" && sess !== undefined) {
            try {
                app.sessions = JSON.parse(sess)
            } catch (e) {
                console.log("sessions parse error: " + e)
            }
        }
        bridge.gui_mark("todos")
        var t = bridge.todo_json()
        if (t !== "" && t !== undefined) {
            try {
                app.todos = JSON.parse(t)
            } catch (e) {
                console.log("todo parse error: " + e)
            }
        }
        bridge.gui_mark("models")
        var md = bridge.models_json()
        if (md !== "" && md !== undefined) {
            try {
                app.models = JSON.parse(md)
            } catch (e) {
                console.log("models parse error: " + e)
            }
        }
        bridge.gui_mark("sounds")
        if (app.sounds.length === 0) {
            var snd = bridge.sounds_json()
            if (snd !== "" && snd !== undefined) {
                try {
                    app.sounds = JSON.parse(snd)
                    if (app.sounds.length > 0
                        && (dingSetting.value === undefined || dingSetting.value === ""))
                        dingSetting.value = app.sounds[0].path
                } catch (e) {
                    console.log("sounds parse error: " + e)
                }
            }
        }
        bridge.gui_mark("cid")
        var cid = bridge.current_session_id()
        if (cid !== undefined && cid !== null && cid !== app.currentSessionId) {
            app.currentSessionId = cid
            // A new session became active — apply the mode saved for it
            // (Build/Plan) if any; sessions without a saved choice live
            // in the server's default mode.
            if (cid !== "" && cid in app.sessionModes
                    && !(cid in app.sessionModesApplied)) {
                app.sessionModesApplied[cid] = true
                bridge.set_mode(cid, app.sessionModes[cid])
            }
        }
        bridge.gui_mark("nav")
        var nav = bridge.take_nav()
        if (nav !== undefined && nav >= 0)
            app.requestPage(nav)
        bridge.gui_mark("voice")
        var v = bridge.voice_status_json()
        if (v !== "" && v !== undefined) {
            try {
                var vo = JSON.parse(v)
                app.voiceModels = vo.models || []
                app.chosenStt = vo.stt || ""
                app.chosenTts = vo.tts || ""
                app.ttsModelReady = app.chosenTts !== ""
                app.voiceLang = vo.lang || "auto"
                if (vo.langs && vo.langs.length) {
                    var opts = [{ id: "auto", name: qsTr("Auto (detect)") }]
                    app.voiceLangOptions = opts.concat(vo.langs)
                }
            } catch (e) {
                console.log("voice parse error: " + e)
            }
        }
        bridge.gui_mark("perms")
        var pj = bridge.permissions_json()
        if (pj !== "" && pj !== undefined) {
            try {
                var parsed = JSON.parse(pj)
                app.pendingPermissions = parsed
                // New requests (not shown yet) — signal the user.
                var fresh = false
                for (var pi = 0; pi < parsed.length; pi++) {
                    var pid = parsed[pi].id
                    if (app.knownPermissionIds.indexOf(pid) < 0) {
                        app.knownPermissionIds = app.knownPermissionIds.concat(pid)
                        fresh = true
                    }
                }
                if (fresh) {
                    app.notifyPermission()
                }
                // The card follows the list: it shows itself while the server
                // waits for an answer and hides itself when the request is
                // gone (answered or withdrawn).
                app.permissionOverlayVisible = parsed.length > 0
                // A settled request clears the reason and the error.
                if (parsed.length === 0) {
                    app.permissionReason = ""
                    app.permissionError = ""
                }
            } catch (e) {
                console.log("permissions parse error: " + e)
            }
        }
        var perr = bridge.permission_error()
        if (perr !== undefined && perr !== null)
            app.permissionError = perr
        var spj = bridge.saved_permissions_json()
        if (spj !== "" && spj !== undefined) {
            try {
                app.savedPermissions = JSON.parse(spj)
            } catch (e) {
                console.log("saved permissions parse error: " + e)
            }
        }
        bridge.gui_mark("forms")
        var fj = bridge.forms_json()
        if (fj !== "" && fj !== undefined) {
            try {
                var fparsed = JSON.parse(fj)
                app.pendingForms = fparsed
                if (fparsed.length === 0) {
                    // The server has no form waiting any more.
                    app.currentFormId = ""
                    app.formOverlayVisible = false
                } else {
                    var fid = fparsed[0].id
                    if (fid !== app.currentFormId) {
                        // A new form — build the answer state and show it.
                        app.currentFormId = fid
                        app.initFormAnswers(fparsed[0])
                        app.formFieldsCache = app.formFieldList(fparsed[0])
                        app.notifyForm()
                        app.formOverlayVisible = true
                    }
                }
            } catch (e) {
                console.log("forms parse error: " + e)
            }
        }
        bridge.gui_mark("settings")
        var sj = bridge.settings_json()
        if (sj !== "" && sj !== undefined) {
            try {
                var sjson = JSON.parse(sj)
                if (sjson.workdir !== undefined && sjson.workdir !== "")
                    app.workdir = sjson.workdir
            } catch (e) {
                console.log("settings parse error: " + e)
            }
        }
        bridge.gui_mark("login")
        var lj = bridge.console_login_json()
        if (lj !== "" && lj !== undefined) {
            try {
                var lo = JSON.parse(lj)
                app.consoleLogin = lo
                // Open the verification page automatically, exactly once per URL.
                if (lo.state === "waiting" && lo.url !== ""
                        && lo.url !== app.consoleLoginOpenedUrl) {
                    app.consoleLoginOpenedUrl = lo.url
                    Qt.openUrlExternally(lo.url)
                }
                if (lo.state !== "waiting")
                    app.consoleLoginOpenedUrl = ""
            } catch (e) {
                console.log("console login parse error: " + e)
            }
        }
        bridge.gui_mark("end")
    }

    function statusColor(s) {
        if (s.indexOf(app.kServerStatusRetryPrefix) === 0) return "#ff9800"
        switch (s) {
        case app.kServerStatusConnecting: return "#2196f3"
        case app.kServerStatusIdle:       return "#4caf50"
        case app.kServerStatusBusy:       return "#ffc107"
        case app.kServerStatusError:      return "#f44336"
        default:                          return Theme.secondaryColor
        }
    }

    // Base status independent of the retry detail token (`retry|…` counts as
    // busy). The worker sets `retry|<next_ms>|<attempt>|<message>` while the
    // server is retrying a failed model call (see main.rs `session.status`).
    function statusBase(s) {
        return s.indexOf(app.kServerStatusRetryPrefix) === 0 ? app.kServerStatusBusy : s
    }

    // Human-readable remaining time: "23h 12m", "4m 05s", "45s".
    function formatRemainingTime(seconds) {
        if (seconds <= 0) return ""
        if (seconds >= 3600) {
            var h = Math.floor(seconds / 3600)
            var m = Math.floor((seconds % 3600) / 60)
            return h + "h " + (m < 10 ? "0" : "") + m + "m"
        }
        if (seconds >= 60) {
            var mins = Math.floor(seconds / 60)
            var secs = seconds % 60
            return mins + "m " + (secs < 10 ? "0" : "") + secs + "s"
        }
        return seconds + "s"
    }

    function send(text) {
        var t = ("" + text).trim()
        if (t === "")
            return
        // A new prompt clears the revert server-side (the reverted tail is
        // dropped on the next turn), so the banner goes away too.
        app.reverted = false
        var acc = app.messages
        acc = acc.concat(app.kTsOpen + Date.now() + app.kTsClose
                        + app.kUserPrefix + t)
        app.messages = acc
        bridge.send_prompt(t)
    }

    // Session mode (build/plan agent) — a property of a particular session. The
    // choice is remembered per session in sessionModes and applied when it is
    // opened (in poll()) or immediately if the session is active.
    function sessionMode(sid) {
        if (sid && sid in app.sessionModes && app.sessionModes[sid])
            return app.sessionModes[sid]
        return "build"
    }
    function setSessionMode(sid, mode) {
        if (!sid || (mode !== "build" && mode !== "plan"))
            return
        app.sessionModes[sid] = mode
        sessionModesSetting.value = JSON.stringify(app.sessionModes)
        sessionModesSetting.sync()
        if (sid === app.currentSessionId) {
            app.sessionModesApplied[sid] = true
            bridge.set_mode(sid, mode)
        }
    }

    function clearHistory() { app.messages = [] }

    // --- Voice module -----------------------------------------------------
    function voiceCmd(cmd, val) {
        bridge.voice_command(cmd, val === undefined ? "" : val)
    }
    function startPttHold() {
        if (!app.recording) {
            // The mic and the speaker must never fight: silence playback now
            // (locally, for instant feedback) and let the worker drop its own
            // queue — VoiceRecordStart makes Rust stop all speech and
            // invalidate anything still synthesising.
            app.stopTtsPlayer()
            app.recognizing = false
            app.recording = true
            app.voiceCmd(app.cmdRecordStart, "")
        }
    }
    function stopPttHold() {
        dictationTimer.stop()
        if (app.recording) {
            // recognizing FIRST, then recording=false: the worker only resumes
            // speech after recognition finishes, so playback cannot start in
            // the gap before STT begins.
            app.recognizing = true
            app.recording = false
            app.voiceCmd(app.cmdRecordStop, "")
        }
    }

    // ── Cover quick actions (обложка) ──────────────────────────────────
    // Ask the chat page to focus the prompt input (the keyboard opens on focus).
    signal quickComposeRequested()
    // Mic dictation from the cover: open the chat page, start recording and
    // let the user speak; after the timeout the dictation stops by itself
    // (then, in voice/PTT modes, the prompt goes out / into the input).
    function startQuickDictation() {
        if (app.recording)
            return
        app.requestPage(2)
        app.startPttHold()
        dictationTimer.restart()
    }
    Timer {
        id: dictationTimer
        interval: 12000
        repeat: false
        onTriggered: app.stopPttHold()
    }
    function voiceModelsFor(engine) {
        var out = []
        for (var i = 0; i < app.voiceModels.length; i++)
            if (app.voiceModels[i].engine === engine)
                out.push(app.voiceModels[i])
        return out
    }
    function modelName(id, list) {
        for (var i = 0; i < list.length; i++)
            if (list[i].model_id === id)
                return list[i].name
        return "—"
    }
    function sttModelName() {
        return app.modelName(app.chosenStt, app.voiceModelsFor(app.engineStt))
    }
    function ttsModelName() {
        return app.modelName(app.chosenTts, app.voiceModelsFor(app.engineTts))
    }
    function humanSize(b) {
        if (!b) return ""
        var g = b / (1024*1024*1024)
        if (g >= 1) return g.toFixed(1) + " GB"
        var m = b / (1024*1024)
        if (m >= 1) return Math.round(m) + " MB"
        return Math.round(b / 1024) + " KB"
    }

    function newSession(title) {
        app.messages = []
        bridge.new_session(title ? title : "")
        app.requestPage(2)
    }

    // Ask for a session name before creating one. Skipping the dialog (or
    // accepting with an empty field) creates the session with a server-generated
    // title, exactly like before.
    function openNewSessionDialog() {
        if (app.pageStack && app.pageStack.currentPage !== null)
            app.pageStack.push(sessionNameDialog, { mode: "new" })
        else
            newSession()
    }

    // Rename an existing session (same dialog, "rename" mode).
    function openRenameSessionDialog(id, title) {
        if (app.pageStack && app.pageStack.currentPage !== null)
            app.pageStack.push(sessionNameDialog, {
                mode: "rename",
                sessionId: id,
                currentTitle: title
            })
        else
            renameSession(id, title)
    }

    function openSession(id) {
        app.messages = []
        bridge.open_session(id)
        app.requestPage(2)
    }

    function openSessionSettings(id) {
        app.messages = []
        bridge.open_session(id)
        app.requestPage(3)
    }

    function renameSession(id, title) {
        bridge.rename_session(id, title)
    }

    function deleteSession(id) {
        bridge.delete_session(id)
    }

    function deleteAllSessions() {
        // Empty new session: reset the feed, otherwise the old history would
        // remain after delete_all (Rust creates the new session asynchronously).
        app.messages = []
        app.currentSessionId = ""
        bridge.delete_all_sessions()
        app.requestPage(2)
    }

    function forkSession(id) {
        app.messages = []
        app.currentSessionId = ""
        bridge.fork_session(id)
    }

    // Fork the current session at a message (long-press menu on a user bubble).
    function forkFrom(msgId) {
        var sid = app.currentSessionId
        if (!msgId || sid === "")
            return
        app.messages = []
        app.currentSessionId = ""
        app.reverted = false
        bridge.fork_from(sid, msgId)
    }

    // Revert the current session tail starting at a user message.
    function revertTo(msgId) {
        var sid = app.currentSessionId
        if (!msgId || sid === "")
            return
        app.messages = []
        bridge.revert_to(sid, msgId)
    }

    // Restore the reverted tail (banner button).
    function unrevertSession() {
        var sid = app.currentSessionId
        if (sid === "")
            return
        app.messages = []
        bridge.unrevert_session(sid)
    }

    function summarizeSession(id) {
        // Compaction streams its summary as a normal assistant bubble now (the
        // worker runs the blocking POST off-task so SSE keeps flowing), and
        // `session.compacted` reloads the feed at the end. Switch to the chat so
        // the live "Compacting…" notice and the growing summary are visible.
        bridge.summarize_session(id)
        app.requestPage(2)
    }
    function abortSession(id) { bridge.abort_session(id) }
    function setModel(id, provider, model) { bridge.set_model(id, provider, model) }

    // The `bridge` object is a dynamic property of this root window, visible only
    // as a bare identifier in THIS file. Pages must go through these wrappers.
    function setWorkdir(path) { bridge.set_workdir(path) }
    function authUrl() { return bridge.auth_url() }
    function providerKeySet() { return bridge.provider_key_set() }
    function saveProviderKey(key) { bridge.save_provider_key(key) }
    function startConsoleLogin() { bridge.start_console_login() }
    function cancelConsoleLogin() { bridge.cancel_console_login() }

    function setShowReasoning(v) {
        app.showReasoning = !!v
        showReasoningSetting.value = app.showReasoning
        showReasoningSetting.sync()
    }

    onInputModeChanged: { inputModeSetting.value = app.inputMode; inputModeSetting.sync() }
    onToolCallsChanged: { toolCallsSetting.value = app.toolCalls; toolCallsSetting.sync() }
    onPttPositionChanged: { pttPositionSetting.value = app.pttPosition; pttPositionSetting.sync() }
    onTtsModeChanged: {
        ttsModeSetting.value = app.ttsMode; ttsModeSetting.sync()
        // Sync the mode with the Rust side (tts_mode), otherwise auto-TTS will not turn on.
        app.voiceCmd(app.cmdTtsMode, app.ttsMode)
    }
    onSoundOnFinishChanged: { soundOnFinishSetting.value = app.soundOnFinish; soundOnFinishSetting.sync() }
    onSoundOnPermissionChanged: { soundOnPermissionSetting.value = app.soundOnPermission; soundOnPermissionSetting.sync() }
    onSendImmediatelyChanged: { sendImmediatelySetting.value = app.sendImmediately; sendImmediatelySetting.sync() }
    onShowReasoningChanged: { showReasoningSetting.value = app.showReasoning; showReasoningSetting.sync() }
    onDingSoundChanged: { if (app.dingSound) dingSetting.value = app.dingSound; dingSetting.sync() }

    // At startup we sync the saved TTS mode (auto/button/off)
    // with the Rust side — otherwise `onTtsModeChanged` will not fire for the initial value.
    Component.onCompleted: {
        // Pull the persisted settings (workdir) once at start: the worker
        // republishes them only when a settings command runs, so without this
        // the UI would keep the QML defaults even though the saved value differs
        // (visually "the setting does not survive a restart"). poll() reads
        // settings_json() and applies it to app.workdir.
        bridge.get_settings()
        app.voiceCmd(app.cmdTtsMode, app.ttsMode)
    }

    function playDing() {
        var slots = [ding1, ding2, ding3, ding4]
        var slot = null
        for (var i = 0; i < slots.length; i++) {
            var s = slots[(app.dingSlot + i) % slots.length]
            if (!s.playing) {
                slot = s
                app.dingSlot = (app.dingSlot + i + 1) % slots.length
                break
            }
        }
        if (!slot) {
            slot = slots[app.dingSlot]
            slot.stop()
            app.dingSlot = (app.dingSlot + 1) % slots.length
        }
        slot.source = app.dingSound
        slot.play()
    }

    function stopDing() {
        var slots = [ding1, ding2, ding3, ding4]
        for (var i = 0; i < slots.length; i++)
            slots[i].stop()
    }

    function setDingSound(path) {
        app.dingSound = path
        dingSetting.value = path
        dingSetting.sync()
    }

    // Preview of the selected tune: first mute all slots, then play
    // with a small pause — an instant play after stop often fails
    // (the sound works "every other time").
    Timer {
        id: dingPreviewTimer
        interval: 80
        onTriggered: app.playDing()
    }
    function previewDing(path) {
        app.setDingSound(path)
        app.stopDing()
        dingPreviewTimer.start()
    }

    function dingSoundName() {
        for (var i = 0; i < app.sounds.length; i++)
            if (app.sounds[i].path === app.dingSound)
                return app.sounds[i].name
        return app.dingSound
    }

    function publishNotification(summary, body) {
        notify.summary = summary
        notify.body = body
        notify.previewSummary = summary
        notify.previewBody = body
        notify.sound = app.dingSound
        notify.publish()
    }

    // Server error: always-visible notification + red bubble in the feed.
    // Does NOT alter daemon connection status (statusText reflects background daemon health).
    function publishError(msg) {
        if (msg === "") msg = qsTr("Server error")
        app.playDing()
        app.publishNotification(qsTr("Server error"), msg)
    }

    // Dev (debug build only): generate a mock server error into the feed.
    function mockServerError(kind) {
        if (app.devTools)
            bridge.mock_error(kind)
    }

    // Dev (only in a debug build): generate a mock agent access request.
    function mockPermission() {
        if (app.devTools)
            bridge.mock_permission()
    }

    function notifyAgentFinished() {
        if (!app.soundOnFinish)
            return
        if (Qt.application.state === Qt.ApplicationActive) {
            app.playDing()
        } else {
            app.publishNotification(qsTr("opencode"), qsTr("Agent finished"))
        }
    }

    function notifyPermission() {
        if (!app.soundOnPermission)
            return
        if (Qt.application.state === Qt.ApplicationActive) {
            app.playDing()
        } else {
            app.publishNotification(qsTr("opencode"), qsTr("Agent requests permission"))
        }
    }

    function notifyForm() {
        if (!app.soundOnFinish)
            return
        if (Qt.application.state === Qt.ApplicationActive) {
            app.playDing()
        } else {
            app.publishNotification(qsTr("opencode"), qsTr("Model asks a question"))
        }
    }
    function stopAgent() {
        if (app.currentSessionId !== "")
            app.abortSession(app.currentSessionId)
    }

    // Answer to an agent permission request: once / always / reject.
    // The card is NOT hidden here: it disappears when the worker confirms the
    // answer (the request leaves the queue). If the answer fails, the card
    // stays and shows the error — so the request can be answered again.
    function answerPermission(session, id, response) {
        bridge.answer_permission(session, id, response, app.permissionReason)
        app.permissionReason = ""
    }

    // Revoke a saved ("always allow") rule.
    function removeSavedPermission(id) {
        bridge.remove_saved_permission(id)
    }

    // The request to show: the one of the session the user is looking at
    // first, the others wait their turn.
    readonly property var currentPermission: {
        var list = app.pendingPermissions
        if (!list || list.length === 0)
            return null
        var cid = app.currentSessionId
        for (var i = 0; i < list.length; i++) {
            if (cid !== "" && list[i].sessionID === cid)
                return list[i]
        }
        return list[0]
    }

    // show/hide the permission request overlay.
    property bool permissionOverlayVisible: false
    function showPermissionOverlay() {
        app.permissionOverlayVisible = app.pendingPermissions.length > 0
    }

    // ── Model forms (opencode v2) ───────────────────────────────────────────
    // The fields to show: the hidden ones are skipped (they are answered with
    // their default). The `when` conditions are checked while rendering.
    function formFieldList(req) {
        var out = []
        var fields = req && req.fields ? req.fields : []
        for (var i = 0; i < fields.length; i++) {
            if (fields[i].hidden === true)
                continue
            out.push(fields[i])
        }
        return out
    }
    // The field the `when` conditions refer to.
    function fieldByKey(key) {
        var fields = app.pendingForms.length > 0 && app.pendingForms[0].fields
                        ? app.pendingForms[0].fields : []
        for (var i = 0; i < fields.length; i++) {
            if (fields[i].key === key)
                return fields[i]
        }
        return undefined
    }
    // Is the field visible? All the `when` conditions must hold; a field whose
    // condition refers to an unanswered field stays hidden (as the server does).
    function fieldActive(field) {
        if (!field || !field.when || field.when.length === 0)
            return true
        for (var i = 0; i < field.when.length; i++) {
            var c = field.when[i]
            var ref = app.fieldByKey(c.key)
            var cur = ref ? app.effectiveAnswer(ref) : undefined
            if (cur === undefined)
                return false
            var eq = cur instanceof Array ? cur.indexOf(c.value) >= 0 : cur === c.value
            if (c.op === "eq" ? !eq : eq)
                return false
        }
        return true
    }
    // The options of a field: a boolean is a Yes/No choice, the others carry
    // their own options.
    function fieldOptions(field) {
        if (!field)
            return []
        if (field.type === "boolean")
            return [{ value: true, label: qsTr("Yes") }, { value: false, label: qsTr("No") }]
        if (field.options && field.options.length > 0)
            return field.options
        return []
    }
    // Does the field offer a list of options?
    function fieldHasOptions(field) {
        return field.type === "string" || field.type === "multiselect"
    }
    // Is the external step confirmed?
    function fieldAcknowledged(field) {
        return app.formAnswers[field.key] === true
    }
    // Fresh answer state for a form: the defaults of the fields (the hidden
    // ones included) and nothing else.
    function initFormAnswers(req) {
        var answers = {}
        var custom = {}
        var fields = req && req.fields ? req.fields : []
        for (var i = 0; i < fields.length; i++) {
            var f = fields[i]
            if (f.type === "external" || f.default === undefined)
                continue
            if (f.type === "multiselect" && f.default instanceof Array) {
                answers[f.key] = f.default.slice(0)
            } else if (f.type === "string" && f.options && f.options.length > 0
                       && typeof f.default === "string") {
                var known = false
                for (var j = 0; j < f.options.length; j++) {
                    if (f.options[j].value === f.default) {
                        known = true
                        break
                    }
                }
                if (known) {
                    answers[f.key] = f.default
                } else {
                    // A default that is not one of the options is an own answer.
                    custom[f.key] = f.default
                }
            } else {
                answers[f.key] = f.default
            }
        }
        app.formAnswers = answers
        app.formCustom = custom
        app.formError = ""
    }
    // The answer of a field as the server wants it (undefined = not answered).
    function effectiveAnswer(field) {
        if (!field)
            return undefined
        var a = app.formAnswers[field.key]
        var c = app.formCustom[field.key]
        if (field.type === "multiselect") {
            var arr = a instanceof Array ? a.slice(0) : []
            if (c !== undefined && c !== "" && arr.indexOf(c) < 0)
                arr.push(c)
            return arr.length > 0 ? arr : undefined
        }
        if (field.type === "string" && field.options && field.options.length > 0) {
            // The own answer wins over the picked option.
            if (c !== undefined && c !== "")
                return c
            return a === undefined ? undefined : a
        }
        if (field.type === "number" || field.type === "integer") {
            if (a === undefined || a === null || a === "")
                return undefined
            var n = Number(a)
            return isNaN(n) ? "" + a : n
        }
        return a
    }
    // Toggle an option of a field (a multiselect keeps several values).
    function toggleOption(field, value) {
        var answers = app.formAnswers
        var copy = {}
        for (var k in answers)
            copy[k] = answers[k]
        if (field.type === "multiselect") {
            var arr = answers[field.key] instanceof Array ? answers[field.key].slice(0) : []
            var idx = arr.indexOf(value)
            if (idx >= 0)
                arr.splice(idx, 1)
            else
                arr.push(value)
            copy[field.key] = arr
        } else {
            copy[field.key] = value
        }
        app.formAnswers = copy
        app.formError = ""
    }
    function isOptionSelected(field, value) {
        var a = app.formAnswers[field.key]
        if (field.type === "multiselect")
            return a instanceof Array && a.indexOf(value) >= 0
        return a === value
    }
    // The typed value of a text/number field.
    function setFieldText(field, text) {
        var answers = app.formAnswers
        var copy = {}
        for (var k in answers)
            copy[k] = answers[k]
        copy[field.key] = text
        app.formAnswers = copy
    }
    // The own answer typed by the user.
    function setFieldCustom(field, text) {
        var custom = app.formCustom
        var copy = {}
        for (var k in custom)
            copy[k] = custom[k]
        copy[field.key] = text
        app.formCustom = copy
    }
    function fieldTextValue(field) {
        var a = app.formAnswers[field.key]
        if (a === undefined || a === null)
            return ""
        return "" + a
    }
    function fieldCustomValue(field) {
        var c = app.formCustom[field.key]
        return c === undefined ? "" : c
    }
    // Open the link of an external step (the user confirms it separately).
    function openFieldUrl(field) {
        if (!field.url || field.url === "")
            return
        Qt.openUrlExternally(field.url)
    }
    // Confirm (or unconfirm) an external step.
    function acknowledgeField(field) {
        var answers = app.formAnswers
        var copy = {}
        for (var k in answers)
            copy[k] = answers[k]
        copy[field.key] = answers[field.key] === true ? false : true
        app.formAnswers = copy
        app.formError = ""
    }
    // Validate one field; returns an error message or an empty string.
    function fieldError(field, value) {
        if (field.type === "external")
            return value === true ? "" : qsTr("Confirm the opened link to continue")
        if (value === undefined)
            return field.required === true ? qsTr("Answer required") : ""
        if (field.required === true
                && (value === "" || (value instanceof Array && value.length === 0)))
            return field.type === "multiselect"
                    ? qsTr("Select at least one option") : qsTr("Answer required")
        if (field.type === "string") {
            if (typeof value !== "string")
                return qsTr("Expected text")
            if (field.minLength !== undefined && value.length < field.minLength)
                return qsTr("Must be at least %1 characters").arg(field.minLength)
            if (field.maxLength !== undefined && value.length > field.maxLength)
                return qsTr("Must be at most %1 characters").arg(field.maxLength)
            if (field.pattern !== undefined && field.pattern !== "") {
                var re = null
                try {
                    re = new RegExp(field.pattern)
                } catch (e) {
                    re = null
                }
                if (re !== null && !re.test(value))
                    return qsTr("Invalid format")
            }
            if (field.format === "email"
                    && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value))
                return qsTr("Expected an email address")
            if (field.format === "uri" && !app.isUrl(value))
                return qsTr("Expected a URL")
            if (field.format === "date" && !app.isDate(value))
                return qsTr("Expected a date (YYYY-MM-DD)")
            if (field.format === "date-time" && isNaN(Date.parse(value)))
                return qsTr("Expected a date and time")
            if (field.options && field.options.length > 0 && field.custom !== true) {
                var found = false
                for (var i = 0; i < field.options.length; i++) {
                    if (field.options[i].value === value) {
                        found = true
                        break
                    }
                }
                if (!found)
                    return qsTr("Select an available option")
            }
            return ""
        }
        if (field.type === "number" || field.type === "integer") {
            if (typeof value !== "number" || !isFinite(value))
                return qsTr("Expected a number")
            if (field.type === "integer" && Math.floor(value) !== value)
                return qsTr("Expected an integer")
            if (typeof field.minimum === "number" && value < field.minimum)
                return qsTr("Must be at least %1").arg(field.minimum)
            if (typeof field.maximum === "number" && value > field.maximum)
                return qsTr("Must be at most %1").arg(field.maximum)
            return ""
        }
        if (field.type === "boolean")
            return typeof value === "boolean" ? "" : qsTr("Expected yes or no")
        if (field.type === "multiselect") {
            if (!(value instanceof Array))
                return qsTr("Expected selections")
            if (field.minItems !== undefined && value.length < field.minItems)
                return qsTr("Select at least %1").arg(field.minItems)
            if (field.maxItems !== undefined && value.length > field.maxItems)
                return qsTr("Select at most %1").arg(field.maxItems)
            if (field.custom !== true && field.options) {
                for (var j = 0; j < value.length; j++) {
                    var ok = false
                    for (var m = 0; m < field.options.length; m++) {
                        if (field.options[m].value === value[j]) {
                            ok = true
                            break
                        }
                    }
                    if (!ok)
                        return qsTr("Select only available options")
                }
            }
        }
        return ""
    }
    function isUrl(s) {
        return /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\/.+/.test(s)
    }
    function isDate(s) {
        if (!/^\d{4}-\d{2}-\d{2}$/.test(s))
            return false
        var d = new Date(s + "T00:00:00.000Z")
        if (isNaN(d.getTime()))
            return false
        return d.toISOString().slice(0, 10) === s
    }
    // Check the whole form; returns the first error or an empty string.
    function validateForm() {
        var req = app.pendingForms.length > 0 ? app.pendingForms[0] : null
        if (!req)
            return qsTr("The form is no longer available")
        var fields = req.fields || []
        for (var i = 0; i < fields.length; i++) {
            var f = fields[i]
            if (f.hidden === true)
                continue
            if (!app.fieldActive(f))
                continue
            var e = app.fieldError(f, app.effectiveAnswer(f))
            if (e !== "")
                return e
        }
        return ""
    }
    // Send the answers to the server (`{fieldKey: value}`).
    function submitForm() {
        var req = app.pendingForms.length > 0 ? app.pendingForms[0] : null
        if (!req)
            return
        var err = app.validateForm()
        if (err !== "") {
            app.formError = err
            return
        }
        var answer = {}
        var fields = req.fields || []
        for (var i = 0; i < fields.length; i++) {
            var f = fields[i]
            if (f.hidden === true) {
                // A hidden field is answered with its default.
                if (f.default !== undefined)
                    answer[f.key] = f.default
                continue
            }
            if (!app.fieldActive(f))
                continue
            var v = app.effectiveAnswer(f)
            if (v !== undefined)
                answer[f.key] = v
        }
        bridge.answer_form(req.sessionID, req.id, JSON.stringify(answer))
        app.formOverlayVisible = false
        app.formError = ""
    }
    // Dismiss the form (the server cancels it).
    function cancelForm() {
        var req = app.pendingForms.length > 0 ? app.pendingForms[0] : null
        if (!req)
            return
        // An empty answer means "cancel".
        bridge.answer_form(req.sessionID, req.id, "")
        app.formOverlayVisible = false
        app.formError = ""
    }

    initialPage: Component {
        Page {
            Connections {
                target: app
                onRequestPage: pager.currentIndex = index
            }

            // Carousel: [0] Global | [1] Sessions | [2] Chat | [3] Session
            // PagedView gives snap and horizontal swipe (snapMode on Flickable is a bug, see memory).
            PagedView {
                id: pager
                anchors.fill: parent
                currentIndex: 2
                wrapMode: PagedView.NoWrap
                model: [globalPage, sessionsPage, chatPage, sessionSettingsPage]

                delegate: Item {
                    width: PagedView.contentWidth
                    height: PagedView.contentHeight

                    Loader {
                        anchors.fill: parent
                        sourceComponent: modelData
                    }
                }

                Component { id: globalPage; GlobalSettingsPage { appWindow: app } }
                Component { id: sessionsPage; SessionsPage { appWindow: app } }
                Component { id: chatPage; ChatPage { appWindow: app } }
                Component { id: sessionSettingsPage; SessionSettingsPage { appWindow: app } }
            }

            // Dot indicator (SFOS 5.1 has no ready-made PageIndicator — we draw it ourselves).
            Row {
                id: dots
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: parent.top
                anchors.topMargin: Theme.paddingMedium
                spacing: Theme.paddingSmall
                visible: pager.count > 1
                opacity: pager.dragging ? 1.0 : Theme.opacityHigh
                z: 10

                Repeater {
                    model: pager.count
                    Rectangle {
                        width: Theme.paddingSmall
                        height: width
                        radius: width / 2
                        color: index === pager.currentIndex ? Theme.primaryColor : Theme.secondaryColor
                    }
                }
            }

            // Agent permission request overlay (on top of the carousel).
            // We put the background dimming in a separate Rectangle WITHOUT opacity
            // on the container: otherwise the text and buttons become transparent.
            Rectangle {
                id: permissionOverlay
                visible: app.permissionOverlayVisible && app.currentPermission !== null
                anchors.fill: parent
                color: Qt.rgba(0, 0, 0, 0.65)
                z: 20

                // The card with the request text is almost opaque so it stays readable.
                Rectangle {
                    id: permissionCard
                    anchors.top: parent.top
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.topMargin: Theme.paddingLarge
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    height: Math.min(permissionColumn.height + 2 * Theme.paddingMedium,
                                     parent.height - 2 * Theme.paddingLarge)
                    color: Qt.rgba(0.05, 0.05, 0.08, 0.95)
                    radius: Theme.paddingMedium
                    border.color: Theme.primaryColor
                    border.width: 1

                    SilicaFlickable {
                        id: permissionFlick
                        anchors.fill: parent
                        anchors.margins: Theme.paddingMedium
                        contentWidth: width
                        contentHeight: permissionColumn.height
                        flickableDirection: Flickable.VerticalFlick
                        clip: true
                        VerticalScrollDecorator {}

                        Column {
                            id: permissionColumn
                            width: parent.width
                            spacing: Theme.paddingMedium

                        Label {
                            text: qsTr("Agent requests permission")
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            color: Theme.primaryColor
                            font.pixelSize: Theme.fontSizeLarge
                            font.bold: true
                            wrapMode: Text.WordWrap
                        }

                        // What is asked: the action and the resources.
                        Label {
                            text: app.currentPermission
                                  ? (app.currentPermission.action || "")
                                    + (app.currentPermission.resources
                                       && app.currentPermission.resources.length > 0
                                       ? (": " + app.currentPermission.resources
                                            .map(function (r) { return r }).join(", "))
                                       : "")
                                  : ""
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            color: Theme.highlightColor
                            wrapMode: Text.WordWrap
                            font.pixelSize: Theme.fontSizeMedium
                        }

                        // The server's own explanation ("Run `rm -rf /`?").
                        Label {
                            visible: app.currentPermission !== null
                                     && app.currentPermission.message !== undefined
                                     && app.currentPermission.message !== ""
                            text: app.currentPermission ? (app.currentPermission.message || "") : ""
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            color: Theme.secondaryColor
                            wrapMode: Text.WordWrap
                            font.pixelSize: Theme.fontSizeSmall
                        }

                        // What will be remembered if the answer is "always".
                        Label {
                            visible: app.currentPermission !== null
                                     && app.currentPermission.save !== undefined
                                     && app.currentPermission.save !== null
                                     && app.currentPermission.save.length > 0
                            text: app.currentPermission
                                  ? qsTr("Will be remembered: %1")
                                    .arg(app.currentPermission.save.join(", "))
                                  : ""
                            anchors.horizontalCenter: parent.horizontalCenter
                            color: Theme.secondaryColor
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            font.pixelSize: Theme.fontSizeSmall
                        }

                        // Which tool asked.
                        Label {
                            visible: app.currentPermission !== null
                                     && app.currentPermission.source !== undefined
                                     && app.currentPermission.source !== null
                                     && app.currentPermission.source.type !== undefined
                            text: app.currentPermission && app.currentPermission.source
                                  ? qsTr("Tool: %1").arg(app.currentPermission.source.id || "")
                                  : ""
                            anchors.horizontalCenter: parent.horizontalCenter
                            color: Theme.secondaryColor
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            font.pixelSize: Theme.fontSizeSmall
                        }

                        // The optional reason: the model sees it with the answer.
                        TextField {
                            id: permissionReasonField
                            width: parent.width
                            placeholderText: qsTr("Reason (optional)")
                            text: app.permissionReason
                            onTextChanged: app.permissionReason = text
                            inputMethodHints: Qt.ImhNoPredictiveText
                        }

                        // The answer did not go through — the card stays, so the
                        // request can be answered again.
                        Label {
                            visible: app.permissionError !== ""
                            text: app.permissionError
                            color: Theme.secondaryColor
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            font.pixelSize: Theme.fontSizeSmall
                        }

                        // The rules the server remembers ("always allow") — they
                        // can be revoked here.
                        Column {
                            width: parent.width
                            spacing: Theme.paddingSmall
                            visible: app.savedPermissions.length > 0

                            Label {
                                text: qsTr("Always allowed:")
                                color: Theme.secondaryColor
                                font.pixelSize: Theme.fontSizeSmall
                                font.bold: true
                            }

                            Repeater {
                                model: app.savedPermissions
                                delegate: Row {
                                    width: parent.width
                                    spacing: Theme.paddingSmall

                                    Label {
                                        width: parent.width - Theme.itemSizeSmall * 2
                                        text: (modelData.action || "")
                                              + (modelData.resource
                                                 ? " / " + modelData.resource : "")
                                        color: Theme.highlightColor
                                        font.pixelSize: Theme.fontSizeSmall
                                        wrapMode: Text.Wrap
                                    }

                                    Button {
                                        text: qsTr("Revoke")
                                        onClicked: app.removeSavedPermission(modelData.id)
                                    }
                                }
                            }
                        }

                        // More requests are waiting.
                        Label {
                            visible: app.pendingPermissions.length > 1
                            text: qsTr("+%1 more waiting").arg(app.pendingPermissions.length - 1)
                            color: Theme.secondaryColor
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            font.pixelSize: Theme.fontSizeSmall
                        }

                        Item {
                            width: parent.width
                            height: Theme.paddingSmall
                        }

                        Button {
                            text: qsTr("Allow once")
                            anchors.horizontalCenter: parent.horizontalCenter
                            enabled: app.currentPermission !== null
                            onClicked: {
                                if (app.currentPermission) {
                                    app.answerPermission(
                                        app.currentPermission.sessionID,
                                        app.currentPermission.id,
                                        app.replyOnce)
                                }
                            }
                        }
                        Button {
                            text: qsTr("Always allow")
                            anchors.horizontalCenter: parent.horizontalCenter
                            enabled: app.currentPermission !== null
                            onClicked: {
                                if (app.currentPermission) {
                                    app.answerPermission(
                                        app.currentPermission.sessionID,
                                        app.currentPermission.id,
                                        app.replyAlways)
                                }
                            }
                        }
                        Button {
                            text: qsTr("Deny")
                            anchors.horizontalCenter: parent.horizontalCenter
                            enabled: app.currentPermission !== null
                            onClicked: {
                                if (app.currentPermission) {
                                    app.answerPermission(
                                        app.currentPermission.sessionID,
                                        app.currentPermission.id,
                                        app.replyReject)
                                }
                            }
                        }
                    }
                }
            }
        }

            // Model form overlay (higher z than the permission card).
            Rectangle {
                id: formOverlay
                visible: app.formOverlayVisible && app.pendingForms.length > 0
                         && app.pendingForms[0] !== undefined
                anchors.fill: parent
                color: Qt.rgba(0, 0, 0, 0.7)
                z: 25

                Rectangle {
                    id: formCard
                    anchors.top: parent.top
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.topMargin: Theme.paddingLarge
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    height: Math.min(formColumn.height + 2 * Theme.paddingMedium,
                                     parent.height - 2 * Theme.paddingLarge)
                    color: Qt.rgba(0.05, 0.05, 0.08, 0.97)
                    radius: Theme.paddingMedium
                    border.color: Theme.primaryColor
                    border.width: 1

                    Flickable {
                        anchors.fill: parent
                        anchors.margins: Theme.paddingMedium
                        contentWidth: width
                        contentHeight: formColumn.height
                        flickableDirection: Flickable.VerticalFlick

                        Column {
                            id: formColumn
                            width: parent.width
                            spacing: Theme.paddingMedium

                            Label {
                                width: parent.width
                                text: app.pendingForms.length > 0
                                      && app.pendingForms[0].title !== undefined
                                      && app.pendingForms[0].title !== ""
                                      ? app.pendingForms[0].title
                                      : qsTr("Model asks a question")
                                color: Theme.primaryColor
                                font.pixelSize: Theme.fontSizeLarge
                                font.bold: true
                                wrapMode: Text.Wrap
                            }

                            Repeater {
                                model: app.formFieldsCache

                                delegate: Column {
                                    id: fieldCol
                                    property var field: modelData
                                    width: parent.width
                                    spacing: Theme.paddingSmall
                                    // A field whose `when` condition does not
                                    // hold is hidden (and not answered).
                                    visible: app.fieldActive(fieldCol.field)

                                    Label {
                                        width: parent.width
                                        visible: text !== ""
                                        text: (fieldCol.field.title !== undefined
                                               && fieldCol.field.title !== ""
                                               ? fieldCol.field.title : fieldCol.field.key)
                                              + (fieldCol.field.required === true ? " *" : "")
                                        color: Theme.primaryColor
                                        font.pixelSize: Theme.fontSizeMedium
                                        font.bold: true
                                        wrapMode: Text.Wrap
                                    }
                                    Label {
                                        width: parent.width
                                        visible: fieldCol.field.description !== undefined
                                                 && fieldCol.field.description !== ""
                                        text: fieldCol.field.description !== undefined
                                              ? fieldCol.field.description : ""
                                        font.pixelSize: Theme.fontSizeSmall
                                        color: Theme.secondaryColor
                                        wrapMode: Text.Wrap
                                    }

                                    // The options of the field (a boolean is a
                                    // Yes/No choice, a multiselect keeps several).
                                    Repeater {
                                        model: app.fieldOptions(fieldCol.field)

                                        BackgroundItem {
                                            width: parent.width
                                            height: optCol.height + Theme.paddingSmall
                                            onClicked: app.toggleOption(fieldCol.field, modelData.value)

                                            Column {
                                                id: optCol
                                                width: parent.width
                                                anchors.verticalCenter: parent.verticalCenter
                                                Label {
                                                    width: parent.width
                                                    text: (app.isOptionSelected(fieldCol.field, modelData.value)
                                                           ? "\u2713  " : "") + modelData.label
                                                    color: app.isOptionSelected(fieldCol.field, modelData.value)
                                                           ? Theme.primaryColor
                                                           : Theme.highlightColor
                                                    font.bold: app.isOptionSelected(fieldCol.field, modelData.value)
                                                    wrapMode: Text.Wrap
                                                }
                                                Label {
                                                    width: parent.width
                                                    visible: modelData.description !== undefined
                                                             && modelData.description !== ""
                                                    text: modelData.description !== undefined
                                                          ? modelData.description : ""
                                                    font.pixelSize: Theme.fontSizeSmall
                                                    color: Theme.secondaryColor
                                                    wrapMode: Text.Wrap
                                                }
                                            }
                                        }
                                    }

                                    // The value of a text/number field.
                                    TextField {
                                        width: parent.width
                                        visible: fieldCol.field.type === "string"
                                                 && !(fieldCol.field.options
                                                      && fieldCol.field.options.length > 0)
                                                 || fieldCol.field.type === "number"
                                                 || fieldCol.field.type === "integer"
                                        placeholderText: fieldCol.field.placeholder !== undefined
                                                         && fieldCol.field.placeholder !== ""
                                            ? fieldCol.field.placeholder : qsTr("Your answer…")
                                        inputMethodHints: fieldCol.field.type === "number"
                                            || fieldCol.field.type === "integer"
                                            ? Qt.ImhFormattedNumbers : Qt.ImhNoPredictiveText
                                        Component.onCompleted: text = app.fieldTextValue(fieldCol.field)
                                        onTextChanged: app.setFieldText(fieldCol.field, text)
                                    }

                                    // The own answer (the field allows a custom value).
                                    TextField {
                                        width: parent.width
                                        visible: fieldCol.field.custom === true
                                                 && (fieldCol.field.type === "string"
                                                     || fieldCol.field.type === "multiselect")
                                        placeholderText: qsTr("Your own answer…")
                                        Component.onCompleted: text = app.fieldCustomValue(fieldCol.field)
                                        onTextChanged: app.setFieldCustom(fieldCol.field, text)
                                    }

                                    // An external step: open the link, then confirm it.
                                    Column {
                                        width: parent.width
                                        spacing: Theme.paddingSmall
                                        visible: fieldCol.field.type === "external"

                                        BackgroundItem {
                                            width: parent.width
                                            height: Theme.itemSizeSmall
                                            onClicked: app.openFieldUrl(fieldCol.field)

                                            Label {
                                                anchors.verticalCenter: parent.verticalCenter
                                                anchors.left: parent.left
                                                anchors.right: parent.right
                                                anchors.leftMargin: Theme.horizontalPageMargin
                                                anchors.rightMargin: Theme.horizontalPageMargin
                                                text: fieldCol.field.url !== undefined
                                                      ? fieldCol.field.url : ""
                                                color: Theme.highlightColor
                                                elide: Text.ElideRight
                                            }
                                        }
                                        BackgroundItem {
                                            width: parent.width
                                            height: Theme.itemSizeSmall
                                            onClicked: app.acknowledgeField(fieldCol.field)

                                            Label {
                                                anchors.verticalCenter: parent.verticalCenter
                                                anchors.left: parent.left
                                                anchors.right: parent.right
                                                anchors.leftMargin: Theme.horizontalPageMargin
                                                anchors.rightMargin: Theme.horizontalPageMargin
                                                text: (app.fieldAcknowledged(fieldCol.field)
                                                       ? "\u2713  " : "")
                                                      + qsTr("I have completed this step")
                                                color: app.fieldAcknowledged(fieldCol.field)
                                                       ? Theme.primaryColor
                                                       : Theme.highlightColor
                                                font.bold: app.fieldAcknowledged(fieldCol.field)
                                                elide: Text.ElideRight
                                            }
                                        }
                                    }
                                }
                            }

                            Label {
                                width: parent.width
                                visible: app.formError !== ""
                                text: app.formError
                                color: "#f44336"
                                font.pixelSize: Theme.fontSizeSmall
                                wrapMode: Text.Wrap
                            }

                            Button {
                                width: parent.width
                                text: qsTr("Answer")
                                onClicked: app.submitForm()
                            }
                            Button {
                                width: parent.width
                                text: qsTr("Cancel")
                                onClicked: app.cancelForm()
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Cover (обложка свернутого приложения) ──────────────────────────
    // Shown on the home screen while the app is backgrounded: the text of the
    // last agent answer plus two quick actions. "＋" — new session; the second
    // action composes a new prompt — its icon follows the input mode:
    //  mic (voice/text+PTT) — start dictation,  pen/text-only — focus the input.
    // Inline component (not a separate file) so the closure over `app`, the
    // theme and `qsTr` all work in the cover context.
    Component {
        id: coverComponent
        CoverBackground {
            id: coverRoot

            // The most recent *agent* answer from the feed as plain text:
            // skips user lines, thinking, errors and service notices; strips
            // light markdown. "" when there is none yet.
            function lastAnswer() {
                var arr = app.messages
                for (var i = arr.length - 1; i >= 0; i--) {
                    var raw = "" + arr[i]
                    var body = app.stripPartId(app.stripTs(raw))
                    if (raw === "" || body === "")
                        continue
                    if (app.lineIs(body, app.kUserPrefix)) continue
                    if (app.lineIs(body, app.kThinkTag)) continue
                    if (app.lineIs(body, app.kErrTag)) continue
                    if (app.lineIs(body, app.kSvcTag)) continue
                    var t = body.replace(/```[\w+-]*\n?/g, "")
                                .replace(/\n```/g, "")
                                .replace(/[`#*_>]/g, "")
                    t = t.replace(/\s+/g, " ").trim()
                    if (t === "")
                        continue
                    return t.length > 180 ? t.substring(0, 180) + "…" : t
                }
                return ""
            }
            // Keep the answer text current while the chat streams.
            Connections {
                target: app
                onMessagesChanged: answerLbl.text = coverRoot.lastAnswer()
            }

            Column {
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                    topMargin: Theme.paddingMedium
                }
                spacing: Theme.paddingSmall

                Label {
                    text: "opencode"
                    font.pixelSize: Theme.fontSizeMedium
                    color: Theme.highlightColor
                }
                Label {
                    text: app.statusBase(app.statusText) === "busy"
                          ? qsTr("Working…") : qsTr("Ready")
                    font.pixelSize: Theme.fontSizeSmall
                    color: app.statusBase(app.statusText) === "busy"
                           ? Theme.highlightColor : Theme.secondaryColor
                }
                Label {
                    id: answerLbl
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    text: coverRoot.lastAnswer()
                    horizontalAlignment: Text.AlignLeft
                }
            }

            CoverActionList {
                // Single action — the icon follows what the user needs most:
                // while TTS is active → stop (silences playback WITHOUT pulling
                // the app to the foreground); otherwise by input mode:
                // text-only → pen (opens the app, focuses the prompt input);
                // voice/text+PTT → mic (starts a dictation).
                CoverAction {
                    iconSource: (app.ttsPlaying || app.ttsSynth)
                                ? "image://theme/icon-m-stop"
                                : (app.inputMode === "text"
                                   ? "image://theme/icon-m-edit"
                                   : "image://theme/icon-m-mic")
                    onTriggered: {
                        if (app.ttsPlaying || app.ttsSynth) {
                            app.stopTts()
                            return
                        }
                        app.activate()
                        if (app.inputMode === "text")
                            app.quickComposeRequested()
                        else
                            app.startQuickDictation()
                    }
                }
            }
        }
    }

    Component {
        id: sessionNameDialogComponent
        Dialog {
            id: nameDlg
            property string mode: "new"
            property string sessionId
            property string currentTitle
            readonly property string acceptLabel: mode === "new" ? qsTr("Create")
                                                                 : qsTr("Rename")

            canAccept: mode === "new" || titleField.text.trim().length > 0
            onAccepted: {
                if (mode === "new")
                    app.newSession(titleField.text.trim())
                else
                    app.renameSession(sessionId, titleField.text.trim())
            }
            onStatusChanged: {
                if (status === PageStatus.Active) {
                    titleField.forceActiveFocus()
                }
            }

            Column {
                width: parent.width

                DialogHeader {
                    acceptText: nameDlg.acceptLabel
                    title: nameDlg.mode === "new" ? qsTr("New session")
                                                  : qsTr("Session name")
                }
                TextField {
                    id: titleField
                    width: parent.width
                    label: nameDlg.mode === "new" ? qsTr("Session name (optional)")
                                                  : qsTr("Session name")
                    placeholderText: nameDlg.mode === "new"
                                     ? qsTr("Leave empty to name automatically")
                                     : ""
                    text: nameDlg.currentTitle ? nameDlg.currentTitle : ""
                    inputMethodHints: Qt.ImhNoPredictiveText
                    EnterKey.enabled: nameDlg.canAccept
                    EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                    EnterKey.onClicked: nameDlg.accept()
                }
            }
        }
    }

    // Session-name dialog, shared by "create new session" and "rename session"
}
