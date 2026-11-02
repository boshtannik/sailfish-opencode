import QtQuick 2.6
import Sailfish.Silica 1.0
import Sailfish.Pickers 1.0
import Nemo.Thumbnailer 1.0
import "../components"
import "../components/MdCache.js" as MdCache

Item {
    id: chatPage

    property var appWindow

    readonly property bool busy: appWindow.statusBase(appWindow.statusText) === "busy"
    // Attached files the current model can read (they make Send usable even
    // with an empty text field).
    readonly property int sendableAttachments: appWindow.attachments.filter(appWindow.attachmentSupported).length

    // ms (unix) → "HH:MM" local time.
    function tsLabel(ms) {
        if (!ms)
            return ""
        var d = new Date(Number(ms))
        var h = d.getHours(), m = d.getMinutes()
        return (h < 10 ? "0" : "") + h + ":" + (m < 10 ? "0" : "") + m
    }

    // Same hue, slightly dulled: saturated atmosphere accents make the white
    // bubble text hard to read, so the user bubble gets a dimmed variant.
    function dimColor(c, f) {
        return Qt.rgba(c.r * f, c.g * f, c.b * f, 1)
    }

    // Message buttons follow the text size setting (Settings → Chat display).
    // Message icons follow Settings → Icon size (not the text size).
    readonly property int msgButtonSize: Math.round(Theme.itemSizeMedium * appWindow.iconScale)
    readonly property int msgIconSize: Math.round(Theme.iconSizeMedium * appWindow.iconScale)
    readonly property int smallButtonSize: Math.round(Theme.itemSizeSmall * appWindow.iconScale)
    readonly property int smallIconSize: Math.round(Theme.iconSizeSmallPlus * appWindow.iconScale)
    // Input bar buttons (mic, send/stop, paperclip) follow Icon size too.
    readonly property int barButtonSize: Math.round(Theme.itemSizeMedium * appWindow.iconScale)
    readonly property int barIconSize: Math.round(Theme.iconSizeMedium * appWindow.iconScale)

    // ── Mini-markdown → HTML (Text.RichText) ──────────────────────────────
    // No third-party libraries: headings, lists, quotes, bold/italic,
    // inline code and ``` fenced blocks. Rendering is basic but readable.

    function mdEscape(s) {
        return String(s)
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
    }

    // HTML-escape a plain text/URL for insertion into a RichText string.
    function htmlEsc(s) {
        return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;")
                        .replace(/>/g, "&gt;").replace(/"/g, "&quot;")
    }

    // Inline markup of a single line (after escaping, no newlines).
    function mdInline(s) {
        // 1. Markdown links [text](url) → placeholders, so the bare-URL
        // detector (step 2) cannot re-wrap the same URL inside an href.
        var ml = []
        s = s.replace(/\[([^\]]+)\]\(([^)\s]+)(?:\s+["'][^"']*["'])?\)/g,
            function (m, t, u) {
                ml.push([t, u])
                return "\u0000" + (ml.length - 1) + "\u0000"
            })
        // 2. Bare web URLs inside a paragraph → clickable links (trailing
        // punctuation stays outside the link).
        s = s.replace(/(^|[\s(])(https?:\/\/[\w\-+~!$&'()*,;=:%@\/.?#[\]\\]+)(?=[\s<)]|$)/g,
            function (m, p, u) {
                return p + '<a href="' + htmlEsc(u) + '" color="#82c4ff">'
                       + htmlEsc(u) + '</a>'
            })
        // 3. Restore markdown links.
        s = s.replace(/\u0000(\d+)\u0000/g, function (m, n) {
            return '<a href="' + htmlEsc(ml[+n][1]) + '" color="#82c4ff">'
                   + htmlEsc(ml[+n][0]) + '</a>'
        })
        // Code; bold; italic (*…* and _…_).
        s = s.replace(/`([^`]+)`/g, "<font face='monospace' color='#e0e0e0'>$1</font>")
        s = s.replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>")
        s = s.replace(/(^|[\s(])_([^_]+)_(?=\s|[)\]!.,:;]|$)/g, "$1<i>$2</i>")
        s = s.replace(/(^|[\s(])[*]([^*]+)[*](?=\s|[)\]!.,:;]|$)/g, "$1<i>$2</i>")
        return s
    }

    // Rendered prose per source text: a delegate re-created while scrolling
    // reuses the HTML instead of re-running the regex pipeline (MdCache.js).
    function mdRender(text) {
        var hit = MdCache.get(text)
        if (hit !== undefined)
            return hit
        var html = chatPage.mdBlock(chatPage.mdEscape(text))
        MdCache.put(text, html)
        return html
    }

    // Block-level per line: headings, lists, quotes, rules, fenced code.
    function mdBlock(s) {
        var out = []
        var lines = s.split("\n")
        var inPre = false
        var preBuf = []
        for (var i = 0; i < lines.length; i++) {
            var l = lines[i]
            var fence = /^```([\w+-]*)\s*$/.exec(l)
            if (fence) {
                if (inPre) {
                    // Line breaks inside <pre> are preserved verbatim (Qt RichText),
                    // code spaces/indentation are not collapsed, the font is monospace.
                    out.push("<pre>" + preBuf.join("\n") + "</pre>")
                    preBuf = []
                    inPre = false
                } else {
                    inPre = true
                }
                continue
            }
            if (inPre) { preBuf.push(l); continue }
            var h = /^(#{1,6})\s+(.*)$/.exec(l)
            if (h) {
                var sz = h[1].length === 1 ? "+3"
                       : h[1].length === 2 ? "+2"
                       : h[1].length === 3 ? "+1" : "+0"
                out.push("<font size=\"" + sz + "\"><b>" + chatPage.mdInline(h[2]) + "</b></font>")
                continue
            }
            var ul = /^[-*]\s+(.*)$/.exec(l)
            if (ul) { out.push("\u2022 " + chatPage.mdInline(ul[1])); continue }
            var ol = /^\s*(\d+)[.)]\s*(.*)$/.exec(l)
            if (ol) { out.push("<b>" + ol[1] + "</b>. " + chatPage.mdInline(ol[2])); continue }
            var qt = /^&gt;\s*(.*)$/.exec(l)
            if (qt) { out.push("&gt; <i>" + chatPage.mdInline(qt[1]) + "</i>"); continue }
            var hr = /^\s*([-*_])\1{2,}\s*$/.exec(l)
            if (hr) { out.push("<hr/>"); continue }
            out.push(chatPage.mdInline(l))
        }
        if (inPre) {
            out.push("<pre>" + preBuf.join("\n") + "</pre>")
        }
        return out.join("<br/>")
    }

    // Dedicated HTML for a code block: monospace, verbatim whitespace/newlines.
    function mdCode(s) {
        return "<pre>" + chatPage.mdEscape(s) + "</pre>"
    }

    // Split a bubble body into display segments so that fenced code blocks and
    // whole-line links become their own blocks (each with a copy button):
    //   {code:false, link:false, text:<prose>, url:"", lang:""}
    //   {code:true,  link:false, text:<code>, url:"", lang:"<fence-lang>"}
    //   {code:false, link:true,  text:<title>, url:<url>, lang:""}
    function splitBody(body) {
        var segs = []
        var prose = []
        function flush() {
            if (prose.length > 0) {
                segs.push({ code: false, link: false, text: prose.join("\n"),
                            url: "", lang: "" })
                prose = []
            }
        }
        // A whole line that is exactly one link: `- [title](url)` / `[title](url)`
        // / `- url` / `url`.
        var linkRe = /^\s*(?:[-*]\s+)?(?:\[([^\]]+)\]\(([^)\s]+)(?:\s+["'][^"']*["'])?\)|(https?:\/\/\S+))\s*$/
        var fenceRe = /^```([\w+-]*)\s*$/
        var inPre = false
        var preBuf = []
        var lines = String(body).split("\n")
        for (var i = 0; i < lines.length; i++) {
            var l = lines[i]
            var fence = fenceRe.exec(l)
            if (fence) {
                if (inPre) {
                    flush()
                    segs.push({ code: true, link: false,
                                text: preBuf.join("\n").replace(/\s+$/, ""),
                                url: "", lang: fence[1] })
                    preBuf = []
                    inPre = false
                } else {
                    flush()
                    inPre = true
                }
                continue
            }
            if (inPre) { preBuf.push(l); continue }
            var lm = linkRe.exec(l)
            if (lm) {
                flush()
                var url = lm[2] !== undefined ? lm[2] : lm[3]
                var title = lm[1] !== undefined ? lm[1] : url
                segs.push({ code: false, link: true, text: title, url: url, lang: "" })
                continue
            }
            // A blank line ends a paragraph: each paragraph is its own segment,
            // so while an answer streams only the paragraph being written is
            // laid out again (one long rich-text block re-laid out the whole
            // answer on every update and made the feed stutter).
            if (l.trim() === "") {
                flush()
                continue
            }
            prose.push(l)
        }
        if (inPre) {
            flush()
            segs.push({ code: true, link: false,
                        text: preBuf.join("\n").replace(/\s+$/, ""),
                        url: "", lang: "" })
        }
        flush()
        return segs
    }

    Connections {
        target: chatPage.appWindow
        onDictated: {
            if (text === "" || text === undefined)
                return
            var cur = input.text
            input.text = (cur.length > 0 ? cur + " " : "") + text
            // Setting `text` resets the caret to 0; put it at the end so the
            // next dictation/send continues from where the user left off.
            input.cursorPosition = input.text.length
            input.focus = true
        }
        // Cover action "pen" (text-only mode): focus the prompt input and
        // open the keyboard right away.
        onQuickComposeRequested: {
            input.forceActiveFocus()
            Qt.inputMethod.show()
        }
    }

    StatusHeader {
        id: header
        appWindow: chatPage.appWindow
        width: parent.width
        anchors.top: parent.top
    }

    // Server-reported retry/rate-limit notice (token quota / retryable errors).
    // Mirrors parsing from StatusHeader.qml and stays visible on scroll.
    readonly property var retryInfo: {
        var aw = chatPage.appWindow
        if (!aw) return null
        var s = aw.statusText ? aw.statusText : ""
        var prefix = aw.kServerStatusRetryPrefix ? aw.kServerStatusRetryPrefix : "retry|"
        if (s.indexOf(prefix) !== 0) return null
        var p = s.substring(prefix.length).split("|")
        if (p.length < 2) return null
        return {
            next: parseInt(p[0], 10),
            attempt: parseInt(p[1], 10),
            message: p.slice(2).join("|")
        }
    }
    property int retryTick: 0
    Timer {
        interval: 1000
        repeat: true
        running: retryInfo !== null
        onTriggered: retryTick++
    }
    readonly property int retrySecondsLeft: {
        if (!retryInfo) return 0
        retryTick
        return Math.max(0, Math.ceil((retryInfo.next - Date.now()) / 1000))
    }
    readonly property string retryRemainingText: {
        if (!retryInfo) return ""
        var aw = chatPage.appWindow
        if (aw && aw.formatRemainingTime)
            return aw.formatRemainingTime(retrySecondsLeft)
        return retrySecondsLeft + "s"
    }
    readonly property string retryBannerText: {
        if (!retryInfo) return ""
        var line = qsTr("retrying")
        if (retrySecondsLeft > 0)
            line += " " + qsTr("in %1").arg(retryRemainingText)
        var tail = qsTr("attempt #%1").arg(retryInfo.attempt)
        var msg = retryInfo.message
        if (msg !== "")
            return msg + " — " + line + " - " + tail
        return line + " - " + tail
    }

    Rectangle {
        id: retryBanner
        anchors {
            top: header.bottom
            left: parent.left
            right: parent.right
        }
        height: visible ? Theme.itemSizeSmall : 0
        visible: retryInfo !== null
        // Warm amber tint with transparency so text stays readable on any theme.
        color: Qt.rgba(1.0, 0.7, 0.25, 0.16)
        border.color: Qt.rgba(1.0, 0.7, 0.25, 0.55)
        border.width: 1
        z: 7

        Label {
            text: retryBannerText
            font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
            color: Theme.primaryColor
            elide: Text.ElideRight
            anchors {
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
        }
    }

    // Compaction activity banner. While the shown session is compacted (manual
    // "Compact" or auto on context overflow) the summary streams as a normal
    // growing bubble; this banner marks the operation (spinner + label) until
    // `session.compacted` reloads the feed.
    Rectangle {
        id: compactionBanner
        anchors {
            top: retryBanner.visible ? retryBanner.bottom : header.bottom
            left: parent.left
            right: parent.right
        }
        height: visible ? Theme.itemSizeSmall : 0
        visible: !retryBanner.visible && (chatPage.appWindow ? chatPage.appWindow.compactionActive : false)
        color: Theme.rgba(Theme.highlightBackgroundColor, 0.9)
        z: 5

        BusyIndicator {
            id: compactionSpinner
            size: BusyIndicatorSize.Small
            running: compactionBanner.visible
            anchors {
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
        }
        Label {
            text: chatPage.appWindow.kCompactingNotice
            font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
            color: Theme.primaryColor
            anchors {
                left: compactionSpinner.right
                leftMargin: Theme.paddingLarge
                verticalCenter: parent.verticalCenter
            }
        }
    }

    SilicaListView {
        id: chatList
        anchors {
            top: compactionBanner.visible ? compactionBanner.bottom : (retryBanner.visible ? retryBanner.bottom : header.bottom)
            left: parent.left
            right: parent.right
            bottom: revertStrip.visible ? revertStrip.top : attachStrip.top
        }
        model: appWindow.chatModel
        clip: true
        spacing: Theme.paddingSmall
        delegate: messageDelegate
        // Follow new messages only while the user is at the bottom. If they
        // scrolled up to read history, the feed keeps growing below without
        // yanking the view. `stickBottom` is re-evaluated on every user scroll
        // (movementEnded) and reset when the session changes.
        property bool stickBottom: true
        // Keep bubbles built a few screens beyond the viewport: creating a
        // delegate parses markdown and lays out rich text, which stalled the
        // flick each time a bubble scrolled into view.
        cacheBuffer: Math.max(0, height * 3)
        // Long answers make tall bubbles on a large screen and the stock flick
        // speed needed many swipes to move through them: allow faster flicks
        // (relative to the platform default, whatever the screen density).
        readonly property real flickBoost: 1.75
        Component.onCompleted: {
            console.log("chat list flick: default max velocity " + maximumFlickVelocity
                        + ", deceleration " + flickDeceleration
                        + ", pixel ratio " + Theme.pixelRatio)
            maximumFlickVelocity = maximumFlickVelocity * flickBoost
        }
        // Following the end: growth of the feed (a new bubble, a streamed
        // paragraph) glides the view down instead of jumping, coalesced to one
        // move per frame. Big jumps (opening a session) go straight to the end.
        // The user touching the list stops following; it resumes when they come
        // back near the bottom or send a message.
        readonly property real bottomSlack: Theme.itemSizeLarge
        function nearEnd() {
            return contentY + height >= originY + contentHeight - bottomSlack
        }
        function followEnd() {
            if (!stickBottom || moving || dragging)
                return
            var target = Math.max(originY, originY + contentHeight - height)
            var delta = target - contentY
            if (Math.abs(delta) < 1)
                return
            if (delta < 0 || delta > height * 1.5) {
                followAnim.stop()
                positionViewAtEnd()
                return
            }
            followAnim.to = target
            followAnim.restart()
        }
        NumberAnimation {
            id: followAnim
            target: chatList
            property: "contentY"
            duration: 180
            easing.type: Easing.OutCubic
        }
        Timer {
            id: followTick
            interval: 16
            onTriggered: chatList.followEnd()
        }
        // New lines arrived while the user reads further up (jump button dot).
        property bool unseen: false
        onStickBottomChanged: if (stickBottom) unseen = false
        onCountChanged: {
            if (!stickBottom)
                unseen = true
            // The user's own new message always brings the feed back to the end.
            if (count > 0) {
                var last = appWindow.chatModel.get(count - 1)
                if (last && appWindow.lineIs(appWindow.stripTs("" + last.text), appWindow.kUserPrefix))
                    stickBottom = true
            }
            if (stickBottom)
                followTick.restart()
        }
        onContentHeightChanged: if (stickBottom) followTick.restart()
        onHeightChanged: if (stickBottom) followTick.restart()
        onMovementStarted: {
            followAnim.stop()
            stickBottom = false
        }
        onMovementEnded: stickBottom = nearEnd()
        onFlickEnded: stickBottom = nearEnd()
    }

    // Jump to the latest message: shown while the user is reading further up;
    // a dot marks content that arrived meanwhile.
    Item {
        id: jumpToEnd
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            bottom: attachStrip.top
            bottomMargin: Theme.paddingLarge
        }
        width: Math.round(Theme.itemSizeMedium * appWindow.fontScale)
        height: width
        z: 7
        visible: opacity > 0
        opacity: (!chatList.stickBottom && chatList.count > 0 && !chatList.nearEnd()
                  && !attachSheet.open) ? 1 : 0
        Behavior on opacity { FadeAnimation { duration: 180 } }
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: Theme.rgba(Theme.highlightDimmerColor, 0.92)
            border.width: 1
            border.color: jumpArea.pressed ? Theme.highlightColor
                                           : Theme.rgba(Theme.highlightColor, 0.6)
        }
        Image {
            anchors.centerIn: parent
            width: Math.round(Theme.iconSizeMedium * appWindow.fontScale)
            height: width
            sourceSize.width: width
            sourceSize.height: height
            source: "image://theme/icon-m-down?" + (jumpArea.pressed ? Theme.highlightColor : Theme.primaryColor)
        }
        Rectangle {
            visible: chatList.unseen
            anchors { top: parent.top; right: parent.right }
            width: Theme.paddingLarge
            height: width
            radius: width / 2
            color: Theme.highlightColor
        }
        MouseArea {
            id: jumpArea
            anchors.fill: parent
            onClicked: {
                chatList.stickBottom = true
                chatList.positionViewAtEnd()
            }
        }
    }

    // Scroll position thumb for the chat. Silica's VerticalScrollDecorator
    // eases its thumb towards the position, so during continuous scrolling it
    // visibly trailed the content. This one is bound straight to the view's
    // visible area and moves in the same frame as the content.
    Item {
        id: chatScrollBar
        anchors {
            top: chatList.top
            bottom: chatList.bottom
            right: chatList.right
        }
        width: Theme.paddingMedium
        z: 6
        readonly property real ratio: Math.min(1, chatList.visibleArea.heightRatio)
        readonly property bool needed: ratio < 1 && chatList.contentHeight > chatList.height
        visible: opacity > 0
        opacity: needed && (chatList.moving || scrollBarLinger.running) ? 1 : 0
        Behavior on opacity { FadeAnimation { duration: 250 } }

        // Keep it up briefly after the movement ends, like the stock decorator.
        Timer { id: scrollBarLinger; interval: 700 }
        Connections {
            target: chatList
            onMovementEnded: scrollBarLinger.restart()
        }

        Rectangle {
            readonly property real minH: Theme.itemSizeSmall
            width: Theme.paddingSmall
            radius: width / 2
            x: parent.width - width - Theme.paddingSmall / 2
            height: Math.max(minH, chatScrollBar.height * chatScrollBar.ratio)
            // Position over the free track, so the thumb reaches both ends
            // even when it is taller than the true visible ratio.
            y: {
                var free = 1 - chatScrollBar.ratio
                var pos = free > 0 ? chatList.visibleArea.yPosition / free : 0
                return Math.max(0, Math.min(1, pos)) * (chatScrollBar.height - height)
            }
            color: Theme.highlightColor
            opacity: 0.8
        }
    }

    // Opening/switching a session jumps back to the bottom (fresh history).
    Connections {
        target: appWindow
        onCurrentSessionIdChanged: chatList.stickBottom = true
    }

    // Dev button that generates mock server errors: to test how errors
    // are displayed in the feed without a real server (see bridge.mock_error).
    // Hidden — not needed in the production UI.
    IconButton {
        id: mockErrBtn
        visible: false
        width: Theme.itemSizeMedium
        height: Theme.itemSizeMedium
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            bottom: inputPanel.top
            bottomMargin: Theme.paddingMedium
        }
        icon.source: "image://theme/icon-m-clear"
        highlighted: mockErrPanel.visible
        onClicked: {
            mockErrBtn.z = mockErrPanel.visible ? 0 : 10
            mockErrPanel.visible = !mockErrPanel.visible
        }
    }

    // Dev panel: buttons to pick a mock error. If not all fit —
    // it scrolls inside (SilicaFlickable + VerticalScrollDecorator).
    Rectangle {
        id: mockErrPanel
        visible: false
        z: 20
        width: Math.min(parent.width - 2 * Theme.horizontalPageMargin,
                        Theme.itemSizeLarge * 7)
        height: Math.min(Theme.itemSizeMedium * 6 + Theme.paddingSmall * 5
                         + 2 * Theme.paddingMedium,   // full height of the list
                         parent.height * 0.5)
        radius: Theme.paddingMedium
        color: Theme.rgba(Theme.overlayBackgroundColor, 0.92)
        border.color: Theme.primaryColor
        border.width: 1
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            bottom: mockErrBtn.top
            bottomMargin: Theme.paddingSmall
        }

        SilicaFlickable {
            id: mockErrFlick
            anchors {
                fill: parent
                margins: Theme.paddingMedium
            }
            contentHeight: mockErrCol.height
            VerticalScrollDecorator {}

            Column {
                id: mockErrCol
                width: parent.width
                spacing: Theme.paddingSmall

                Button {
                    width: parent.width
                    text: "ProviderAuthError"
                    onClicked: { appWindow.mockServerError("ProviderAuthError"); mockErrPanel.visible = false }
                }
                Button {
                    width: parent.width
                    text: "API Error 401"
                    onClicked: { appWindow.mockServerError("APIError401"); mockErrPanel.visible = false }
                }
                Button {
                    width: parent.width
                    text: "API Error 429 (quota)"
                    onClicked: { appWindow.mockServerError("APIError429"); mockErrPanel.visible = false }
                }
                Button {
                    width: parent.width
                    text: "MessageOutputLength"
                    onClicked: { appWindow.mockServerError("MessageOutputLengthError"); mockErrPanel.visible = false }
                }
                Button {
                    width: parent.width
                    text: "MessageAborted"
                    onClicked: { appWindow.mockServerError("MessageAbortedError"); mockErrPanel.visible = false }
                }
                Button {
                    width: parent.width
                    text: "UnknownError"
                    onClicked: { appWindow.mockServerError("UnknownError"); mockErrPanel.visible = false }
                }
                // Separator before the "non-standard" events.
                Rectangle {
                    width: parent.width
                    height: 1
                    color: Theme.rgba(Theme.primaryColor, 0.25)
                }
                Button {
                    width: parent.width
                    text: "Permission ask (bash)"
                    onClicked: { appWindow.mockPermission(); mockErrPanel.visible = false }
                }
                Button {
                    width: parent.width
                    text: "Burst: 3 errors at once"
                    onClicked: {
                        appWindow.mockServerError("ProviderAuthError")
                        appWindow.mockServerError("APIError429")
                        appWindow.mockServerError("UnknownError")
                        mockErrPanel.visible = false
                    }
                }
            }
        }
    }

    Component {
        id: messageDelegate

        Item {
            id: line
            width: chatList.width
            // Model "thinking": a binding to the "show thinking" switch
            // hides/shows already rendered [[think]] lines at once, without
            // reopening the session (new ones are not accumulated when the setting is off —
            // poll() in the root QML filters them out).
            // Inline tool rows obey the "Tool calls" setting the same way
            // (hide mode drops them immediately).
            visible: (appWindow.showReasoning || !line.isThink)
                     && (appWindow.toolCalls !== "hide" || !line.isTool)
            // The long-press menu is pinned to the item's bottom; Silica clips
            // it to the item's height (layer sourceRect, see private/ViewItem.qml).
            // Grow the item by the menu height while it is open so every menu
            // entry fits (without this only the first entry was visible on
            // short bubbles).
            // The menu is created on first long-press only: a ContextMenu per
            // bubble was a large share of each delegate's creation cost.
            property Item userMenu: null
            property bool menuOpen: line.userMenu !== null && line.userMenu._open
            height: visible ? (line.isTool
                               ? toolRow.height + Theme.paddingSmall
                               : (line.isModelSwitch
                                  ? switchRow.height
                                  : bubble.height + Theme.paddingSmall
                                    + (line.menuOpen ? line.userMenu.height : 0))) : 0

            // Raw line from the worker: `[[t:<ms>]]>>> text` (user) or
            // `[[t:<ms>]]text` (agent); a live user line from `send()` — `[[t:<ms>]]>>> text`.
            property string raw: "" + model.text
            property string tsMs: {
                var m = appWindow.tsRe.exec(line.raw)
                return m ? m[1] : ""
            }
            // User lines are marked with the ">>> " prefix AFTER the timestamp.
            property bool isUser: appWindow.lineIs(appWindow.stripTs(line.raw),
                                                  appWindow.kUserPrefix)
            // Model "thinking": the line `[[t:<ms>]][[think]]<text>` before the answer.
            // Rendered as recessed italic; TTS/the button are unavailable for it.
            property bool isThink: appWindow.lineIs(appWindow.stripTs(line.raw),
                                                   appWindow.kThinkTag)
            // Server/service error (limits, AI access, etc.) — red-ish bubble.
            // Worker sends `[[e:KEY…]]`; QML stores the line as `[[e]]<text>`.
            property bool isError: appWindow.lineIs(appWindow.stripTs(line.raw),
                                                    appWindow.kErrTag)
            // Service notice (voice/STT/TTS/catalog progress) — a SYSTEM bubble:
            // muted styling, no speak button, no auto-TTS (never an answer).
            property bool isSvc: appWindow.lineIs(appWindow.stripTs(line.raw),
                                                 appWindow.kSvcTag)
            // Inline tool call (TUI-style): `[[tool]]<callID>\u001f<name>\u001f
            // <status>\u001f<text>`. Rendered as a compact row, not a bubble.
            property bool isTool: appWindow.lineIs(appWindow.stripTs(line.raw),
                                                   appWindow.kToolTag)
            property bool isSystem: line.isError || line.isSvc
            // "Switched to <model>" divider (see kModelSwitchTag).
            property bool isModelSwitch: appWindow.lineIs(appWindow.stripTs(line.raw),
                                                          appWindow.kModelSwitchTag)
            // The model's own answer (not thinking, not a tool row, not a notice).
            property bool isAnswer: !line.isUser && !line.isThink && !line.isSystem && !line.isTool
                                    && !line.isModelSwitch
            // Thinking bubbles start compact (last lines only) unless the user
            // chose "Expand thinking"; tapping the caption toggles one bubble.
            property bool thinkOpen: appWindow.expandThinking
            readonly property bool thinkCollapsed: line.isThink && !line.thinkOpen
            readonly property real collapsedThinkHeight: 3 * appWindow.fs(Theme.fontSizeSmall) * 1.45
            // Tool-row fields (empty for non-tool lines).
            property string toolName: appWindow.toolField(line.raw, 1)
            property string toolStatus: appWindow.toolField(line.raw, 2)
            property string toolText: appWindow.toolField(line.raw, 3)
            // Bold tool name + its argument, both HTML-escaped (RichText).
            property string toolHtml: {
                var h = "<b>" + chatPage.htmlEsc(line.toolName) + "</b>"
                if (appWindow.toolCalls !== "minimal" && line.toolText !== "")
                    h += " " + chatPage.htmlEsc(line.toolText)
                return h
            }
            property string body: {
                var b = appWindow.stripTs(line.raw)
                if (appWindow.lineIs(b, appWindow.kThinkTag))
                    b = b.substring(appWindow.kThinkTag.length)
                if (appWindow.lineIs(b, appWindow.kUserPrefix))
                    b = b.substring(appWindow.kUserPrefix.length)
                if (line.isError)
                    b = b.substring(appWindow.kErrTag.length)
                if (line.isSvc)
                    b = b.substring(appWindow.kSvcTag.length)
                b = appWindow.stripMsgId(b)
                b = appWindow.stripPartId(b)
                b = b.replace(/^\n+/, "")
                if (b.length > appWindow.kFeedBodyCap)
                    b = b.substring(0, appWindow.kFeedBodyCap) + "\n… [text truncated]"
                return b
            }
            // The body split into display segments: prose, fenced-code blocks
            // and whole-line links — so copy buttons can sit next to each block.
            // `{code, link, text, url, lang}` (see chatPage.splitBody).
            property var segments: (line.isTool || line.isModelSwitch) ? [] : chatPage.splitBody(line.body)
            // Segments mirrored into a ListModel in place: while an answer
            // streams, only the segment that grew is laid out again. A plain
            // array model rebuilt every segment (and its rich text) on every
            // update, which stalled scrolling during generation.
            ListModel { id: segModel }
            function syncSegments() {
                var s = line.segments
                while (segModel.count > s.length)
                    segModel.remove(segModel.count - 1)
                for (var i = 0; i < s.length; i++) {
                    var it = { code: !!s[i].code, link: !!s[i].link, text: s[i].text || "",
                               url: s[i].url || "", lang: s[i].lang || "" }
                    if (i >= segModel.count) {
                        segModel.append(it)
                        continue
                    }
                    var cur = segModel.get(i)
                    if (cur.code !== it.code || cur.link !== it.link) {
                        segModel.set(i, it)
                        continue
                    }
                    if (cur.text !== it.text) segModel.setProperty(i, "text", it.text)
                    if (cur.url !== it.url) segModel.setProperty(i, "url", it.url)
                    if (cur.lang !== it.lang) segModel.setProperty(i, "lang", it.lang)
                }
            }
            onSegmentsChanged: syncSegments()
            Component.onCompleted: syncSegments()
            // Server messageID of this bubble (user lines only): enables the
            // long-press revert/fork menu. Empty for agent/system bubbles.
            property string msgId: appWindow.msgIdOf(line.raw)
            readonly property real maxW: chatList.width - 2 * Theme.horizontalPageMargin

            Rectangle {
                id: bubble
                visible: !line.isTool && !line.isModelSwitch
                radius: Theme.paddingMedium
                color: line.isThink
                       ? Theme.rgba(Theme.primaryColor, 0.05)
                       : (line.isUser
                          ? chatPage.dimColor(Theme.highlightColor, 0.80)
                          : (line.isError
                             ? Qt.rgba(0.85, 0.15, 0.15, 0.18)
                             : (line.isSvc
                                ? Theme.rgba(Theme.secondaryColor, 0.10)
                                : Theme.rgba(Theme.primaryColor, 0.12))))
                width: line.maxW
                height: bodyClip.y + bodyClip.height
                        + (footRow.height > 0 ? footRow.height + Theme.paddingSmall : 0)
                        + Theme.paddingSmall
                anchors.right: line.isUser ? parent.right : undefined
                anchors.left: line.isUser ? undefined : parent.left
                anchors.rightMargin: line.isUser ? Theme.horizontalPageMargin : 0
                anchors.leftMargin: line.isUser ? 0 : Theme.horizontalPageMargin

                // Thinking is marked by an accent bar on its left edge.
                Rectangle {
                    visible: line.isThink
                    x: Theme.paddingSmall / 2
                    y: bubble.radius
                    width: Math.max(2, Theme.paddingSmall / 2)
                    height: bubble.height - 2 * bubble.radius
                    radius: width / 2
                    color: Theme.secondaryHighlightColor
                    opacity: 0.8
                }

                // Caption: "Thinking ▸/▾" (tap to expand) or "Answer".
                Item {
                    id: caption
                    readonly property bool shown: line.isThink || (line.isAnswer && appWindow.labelRoles)
                    visible: shown
                    x: bodyClip.x
                    y: Theme.paddingSmall
                    width: bodyClip.width
                    height: shown ? captionLbl.height + Theme.paddingSmall : 0
                    Label {
                        id: captionLbl
                        text: line.isThink
                              ? qsTr("Thinking") + (line.thinkOpen ? "  ▾" : "  ▸")
                              : qsTr("Answer")
                        font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
                        font.bold: true
                        color: line.isThink ? Theme.secondaryHighlightColor : Theme.highlightColor
                    }
                    MouseArea {
                        // Generous touch target around the small caption.
                        anchors.fill: parent
                        anchors.margins: -Theme.paddingMedium
                        enabled: line.isThink
                        onClicked: line.thinkOpen = !line.thinkOpen
                    }
                }

                // Collapsed thinking shows its last lines (the live tail while
                // it streams); expanded shows everything.
                Item {
                    id: bodyClip
                    x: Theme.paddingSmall + (line.isThink ? Theme.paddingSmall : 0)
                    y: caption.y + caption.height
                    width: bubble.width - Theme.paddingSmall - x
                    height: line.thinkCollapsed ? Math.min(bodyCol.height, line.collapsedThinkHeight)
                                                : bodyCol.height
                    clip: line.thinkCollapsed && bodyCol.height > line.collapsedThinkHeight

                Column {
                    id: bodyCol
                    y: line.thinkCollapsed ? Math.min(0, bodyClip.height - bodyCol.height) : 0
                    width: bodyClip.width
                    spacing: Theme.paddingSmall

                    Repeater {
                        model: segModel
                        // Each segment builds only the layout of its own kind.
                        // Before, every segment created the prose label, the link
                        // row and a whole code card (flickable, two buttons,
                        // scroll decorator) and hid two of them, which made each
                        // bubble cost dozens of objects as it scrolled into view.
                        delegate: Loader {
                            id: segItem
                            // Role object of segModel: code/link/text/url/lang.
                            property var seg: model
                            sourceComponent: segItem.seg.code ? codeSegment
                                             : (segItem.seg.link ? linkSegment : proseSegment)

                            // Free-form prose (markdown → rich text).
                            Component {
                                id: proseSegment
                                Label {
                                    width: bodyCol.width
                                    text: chatPage.mdRender(segItem.seg.text)
                                    textFormat: Text.RichText
                                    wrapMode: Text.Wrap
                                    onLinkActivated: Qt.openUrlExternally(link)
                                    font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
                                    font.italic: line.isThink
                                    color: (line.isThink || line.isSvc)
                                           ? Theme.secondaryColor : Theme.primaryColor
                                }
                            }

                            // Whole-line link: label + copy button. The button is pinned to the row's
                            // right edge and the label is cut off at the icon's edge —
                            // the icon can never sit ON the text.
                            Component {
                                id: linkSegment
                                Item {
                                    id: linkRow
                                    width: bodyCol.width
                                    height: Math.max(urlLbl.height, copyLinkBtn.height)
                                    Label {
                                        id: urlLbl
                                        anchors.left: parent.left
                                        anchors.right: copyLinkBtn.left
                                        anchors.rightMargin: Theme.paddingSmall
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: segItem.seg.text || segItem.seg.url
                                        wrapMode: Text.Wrap
                                        clip: true
                                        font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
                                        font.family: "monospace"
                                        color: Theme.highlightColor
                                    }
                                    // Tap the URL itself to open it (the copy button
                                    // sits to the right).
                                    MouseArea {
                                        anchors.fill: urlLbl
                                        onClicked: Qt.openUrlExternally(segItem.seg.url)
                                    }
                                    IconButton {
                                        id: copyLinkBtn
                                        width: chatPage.smallButtonSize
                                        height: chatPage.smallButtonSize
                                        anchors.right: parent.right
                                        anchors.verticalCenter: parent.verticalCenter
                                        icon.source: "image://theme/icon-m-clipboard"
                                        icon.width: chatPage.smallIconSize
                                        icon.height: chatPage.smallIconSize
                                        icon.sourceSize.width: chatPage.smallIconSize
                                        icon.sourceSize.height: chatPage.smallIconSize
                                        onClicked: appWindow.copyText(segItem.seg.url)
                                    }
                                }
                            }

                            // Fenced code block: own card with a light header bar
                            // and a copy button in its top-right corner.
                            Component {
                                id: codeSegment
                                Rectangle {
                                    id: segBox
                                    width: bodyCol.width
                                    radius: Theme.paddingSmall
                                    color: Theme.rgba(Theme.highlightColor, 0.08)
                                    height: codeHeader.height
                                            + codeLbl.height + 2 * Theme.paddingSmall
                                    Rectangle {
                                        id: codeHeader
                                        width: parent.width
                                        height: chatPage.smallButtonSize
                                        radius: Theme.paddingSmall
                                        color: Theme.rgba(Theme.highlightColor, 0.15)
                                        Label {
                                            anchors.left: parent.left
                                            anchors.leftMargin: Theme.paddingSmall
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: segItem.seg.lang
                                            visible: segItem.seg.lang.length > 0
                                            color: Theme.secondaryColor
                                            font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
                                        }
                                        IconButton {
                                            id: copyCodeBtn
                                            anchors.right: parent.right
                                            anchors.rightMargin: Theme.paddingSmall
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: chatPage.smallButtonSize
                                            height: chatPage.smallButtonSize
                                            icon.source: "image://theme/icon-m-clipboard"
                                            icon.width: chatPage.smallIconSize
                                            icon.height: chatPage.smallIconSize
                                            icon.sourceSize.width: chatPage.smallIconSize
                                            icon.sourceSize.height: chatPage.smallIconSize
                                            onClicked: appWindow.copyText(segItem.seg.text)
                                        }
                                    }
                                    SilicaFlickable {
                                        id: codeFlick
                                        anchors.left: parent.left
                                        anchors.right: parent.right
                                        anchors.top: codeHeader.bottom
                                        height: Math.max(Theme.itemSizeMedium,
                                                         codeLbl.paintedHeight + 2 * Theme.paddingSmall)
                                        contentWidth: codeLbl.paintedWidth + 2 * Theme.paddingSmall
                                        contentHeight: height
                                        clip: true
                                        flickableDirection: Flickable.HorizontalFlick
                                        Label {
                                            id: codeLbl
                                            x: Theme.paddingSmall
                                            y: Theme.paddingSmall
                                            text: chatPage.mdCode(segItem.seg.text)
                                            textFormat: Text.RichText
                                            wrapMode: Text.NoWrap
                                            font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
                                            color: Theme.primaryColor
                                        }
                                        HorizontalScrollDecorator {}
                                    }
                                }
                            }
                        }
                    }
                }
                }

                // Bottom line: time (+ copy and, for the agent, the TTS button).
                // Pinned to its own bubble edge, with padding from the edges.
                Row {
                    id: footRow
                    visible: line.tsMs !== ""
                    anchors.right: line.isUser ? undefined : bubble.right
                    anchors.left: line.isUser ? bubble.left : undefined
                    anchors.rightMargin: Theme.paddingSmall + 1
                    anchors.leftMargin: Theme.paddingSmall
                    y: bodyClip.y + bodyClip.height + Theme.paddingSmall
                    spacing: Theme.paddingMedium

                    // 📋 copy this bubble to the clipboard — the agent's answer
                    // (or thinking) and the user's own messages alike.
                    // Created only when enabled (Settings → Chat display).
                    Loader {
                        active: !line.isSystem && appWindow.showCopyButton
                        visible: active
                        sourceComponent: Component {
                            IconButton {
                                width: chatPage.msgButtonSize
                                height: chatPage.msgButtonSize
                                icon.source: "image://theme/icon-m-clipboard"
                                icon.width: chatPage.msgIconSize
                                icon.height: chatPage.msgIconSize
                                icon.sourceSize.width: chatPage.msgIconSize
                                icon.sourceSize.height: chatPage.msgIconSize
                                onClicked: appWindow.copyText(line.body)
                            }
                        }
                    }

                    // 🔊 speak this bubble: agent answers only (not user).
                    // Hit area — the full `iconSizeMedium` icon size, so
                    // it is easy to hit with a finger.
                    // When THIS VERY bubble is being synthesised/spoken, the button
                    // turns into "stop" (stop icon + loader during synthesis),
                    // so TTS can be interrupted right on the bubble itself, not via a separate
                    // button below.
                    Loader {
                        active: !line.isUser && !line.isSystem
                                && appWindow.showSpeakButton
                                && appWindow.ttsModelReady
                                && appWindow.ttsMode !== "off"
                        visible: active
                        sourceComponent: speakButtonComponent
                    }
                    Component {
                        id: speakButtonComponent
                    IconButton {
                        id: speakBtn
                        width: chatPage.msgButtonSize
                        height: chatPage.msgButtonSize
                        icon.width: chatPage.msgIconSize
                        icon.height: chatPage.msgIconSize
                        icon.sourceSize.width: chatPage.msgIconSize
                        icon.sourceSize.height: chatPage.msgIconSize
                        property bool active: appWindow.speakingBubble === line.body
                                              && (appWindow.ttsSynth || appWindow.ttsPlaying)
                        icon.source: active
                                     ? "image://theme/icon-m-stop"
                                     : "image://theme/icon-m-speaker-on"
                        highlighted: active
                        // While this bubble is being synthesised — a spinner over
                        // the icon. Created only then: a BusyIndicator per bubble
                        // added to every bubble's creation cost while scrolling.
                        Loader {
                            anchors.centerIn: parent
                            active: appWindow.ttsSynth
                                    && appWindow.speakingBubble === line.body
                            sourceComponent: Component {
                                BusyIndicator { running: true }
                            }
                        }
                        onClicked: {
                            if (active)
                                appWindow.stopTts()
                            else
                                appWindow.speakText(line.body)
                        }
                    }
                    }

                    Label {
                        id: tsLabel
                        text: chatPage.tsLabel(line.tsMs)
                        anchors.verticalCenter: parent.verticalCenter
                        font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
                        color: Theme.secondaryColor
                    }
                }

                // Long-press a user bubble → edit/fork menu (mirror of the TUI).
                // Disabled while the agent is busy: reverting needs an idle session.
                MouseArea {
                    anchors.fill: parent
                    enabled: line.isUser && line.msgId !== "" && !chatPage.busy
                    onPressAndHold: {
                        if (line.userMenu === null)
                            line.userMenu = userMenuComponent.createObject(line)
                        line.userMenu.open(line)
                    }
                }
            }

            // Revert/fork actions for this user message. ContextMenu reparents
            // itself to the delegate item when opened (Silica ContextMenu.open).
            Component {
                id: userMenuComponent
                ContextMenu {
                MenuItem {
                    // TUI parity: revert to this message AND put its text back
                    // into the prompt field, so it can be edited and re-sent.
                    text: qsTr("Edit")
                    onClicked: {
                        input.text = line.body
                        input.cursorPosition = input.text.length
                        input.forceActiveFocus()
                        appWindow.revertTo(line.msgId)
                    }
                }
                MenuItem {
                    text: qsTr("Fork")
                    onClicked: appWindow.forkFrom(line.msgId)
                }
                }
            }

            // Model switch divider: ── Switched to <model> ──
            Item {
                id: switchRow
                visible: line.isModelSwitch
                width: chatList.width
                height: visible ? switchLbl.height + 2 * Theme.paddingMedium : 0
                Rectangle {
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.horizontalPageMargin
                    anchors.right: switchLbl.left
                    anchors.rightMargin: Theme.paddingMedium
                    anchors.verticalCenter: parent.verticalCenter
                    height: 1
                    color: Theme.rgba(Theme.secondaryHighlightColor, 0.4)
                }
                Label {
                    id: switchLbl
                    anchors.centerIn: parent
                    width: Math.min(implicitWidth, parent.width - 4 * Theme.horizontalPageMargin)
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: line.isModelSwitch ? appWindow.modelSwitchText(line.raw) : ""
                    font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
                    color: Theme.secondaryHighlightColor
                }
                Rectangle {
                    anchors.left: switchLbl.right
                    anchors.leftMargin: Theme.paddingMedium
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    height: 1
                    color: Theme.rgba(Theme.secondaryHighlightColor, 0.4)
                }
            }

            // Inline tool row (TUI-style): a status dot + bold tool name + its
            // argument, wrapped with a hanging indent under the content (the
            // label starts after the dot, so wrapped lines align with the text).
            Item {
                id: toolRow
                visible: line.isTool
                width: line.maxW
                height: toolLbl.height
                anchors.left: parent.left
                anchors.leftMargin: Theme.horizontalPageMargin

                Rectangle {
                    id: toolDot
                    width: Theme.paddingSmall
                    height: width
                    radius: width / 2
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.topMargin: Theme.paddingSmall
                    color: line.toolStatus === appWindow.kStatusError
                           ? Qt.rgba(0.85, 0.15, 0.15, 1)
                           : (line.toolStatus === appWindow.kStatusCompleted
                              ? Theme.secondaryColor
                              : Theme.highlightColor)
                }
                Label {
                    id: toolLbl
                    anchors.left: toolDot.right
                    anchors.leftMargin: Theme.paddingSmall
                    anchors.right: parent.right
                    anchors.top: parent.top
                    text: line.toolHtml
                    textFormat: Text.RichText
                    wrapMode: Text.Wrap
                    font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
                    color: Theme.secondaryColor
                }
            }
        }
    }

    // ── Attachments for the next prompt ──────────────────────────────────
    // One chip per picked file (✕ removes it). A file the current model
    // cannot read stays listed, marked, and is not sent.
    Item {
        id: attachStrip
        anchors {
            left: parent.left
            right: parent.right
            bottom: inputPanel.top
        }
        visible: appWindow.attachments.length > 0
        height: visible ? attachFlow.height + 2 * Theme.paddingSmall : 0

        Flow {
            id: attachFlow
            x: Theme.horizontalPageMargin
            y: Theme.paddingSmall
            width: parent.width - 2 * Theme.horizontalPageMargin
            spacing: Theme.paddingSmall
            Repeater {
                model: appWindow.attachments
                Loader {
                    readonly property var att: modelData
                    readonly property int attIndex: index
                    readonly property bool ok: appWindow.attachmentSupported(modelData)
                    sourceComponent: (modelData.kind === appWindow.kInputImage
                                      || modelData.kind === appWindow.kInputVideo)
                                     ? thumbTile : fileChip
                }
            }
        }
    }

    // Image/video attachment: a thumbnail tile with a ✕ badge. Tap an image
    // to see it full screen, a video to play it.
    Component {
        id: thumbTile
        BackgroundItem {
            id: thumbItem
            readonly property var att: parent ? parent.att : null
            readonly property int attIndex: parent ? parent.attIndex : -1
            readonly property bool ok: parent ? parent.ok : true
            width: Theme.itemSizeExtraLarge
            height: width
            Thumbnail {
                id: thumb
                anchors.fill: parent
                source: thumbItem.att ? "file://" + thumbItem.att.path : ""
                mimeType: thumbItem.att && thumbItem.att.kind === appWindow.kInputVideo ? "video/mp4" : ""
                sourceSize.width: width
                sourceSize.height: height
                fillMode: Thumbnail.PreserveAspectCrop
                opacity: thumbItem.ok ? 1 : 0.4
            }
            Rectangle {
                anchors.fill: parent
                color: "transparent"
                radius: Theme.paddingSmall
                border.width: thumbItem.ok ? 1 : 2
                border.color: thumbItem.ok ? Theme.rgba(Theme.highlightColor, 0.6) : Theme.errorColor
            }
            Image {
                visible: thumbItem.att && thumbItem.att.kind === appWindow.kInputVideo
                anchors.centerIn: parent
                source: "image://theme/icon-m-play?white"
            }
            // ✕: remove this attachment.
            MouseArea {
                anchors {
                    top: parent.top
                    right: parent.right
                }
                width: Theme.itemSizeExtraSmall
                height: width
                onClicked: appWindow.removeAttachment(thumbItem.attIndex)
                Rectangle {
                    anchors {
                        top: parent.top
                        right: parent.right
                        margins: Theme.paddingSmall / 2
                    }
                    width: Theme.iconSizeSmall + Theme.paddingSmall
                    height: width
                    radius: width / 2
                    color: Theme.rgba("black", 0.65)
                    Label {
                        anchors.centerIn: parent
                        text: "✕"
                        color: "white"
                        font.pixelSize: Theme.fontSizeExtraSmall
                    }
                }
            }
            onClicked: {
                if (!thumbItem.att)
                    return
                if (thumbItem.att.kind === appWindow.kInputVideo)
                    Qt.openUrlExternally("file://" + thumbItem.att.path)
                else
                    pageStack.push(Qt.resolvedUrl("ImageViewerPage.qml"),
                                   { appWindow: appWindow, path: thumbItem.att.path,
                                     name: thumbItem.att.name, index: thumbItem.attIndex })
            }
        }
    }

    // Other attachments (PDF…): a name chip; tap removes it.
    Component {
        id: fileChip
        BackgroundItem {
            id: chip
            readonly property var att: parent ? parent.att : null
            readonly property int attIndex: parent ? parent.attIndex : -1
            readonly property bool ok: parent ? parent.ok : true
            width: Math.min(attachFlow.width, chipRow.width + 2 * Theme.paddingMedium)
            height: Theme.itemSizeExtraSmall
            Rectangle {
                anchors.fill: parent
                radius: height / 2
                color: "transparent"
                border.width: 1
                border.color: chip.ok ? Theme.rgba(Theme.highlightColor, 0.6)
                                      : Theme.rgba(Theme.errorColor, 0.8)
            }
            Row {
                id: chipRow
                anchors.centerIn: parent
                spacing: Theme.paddingSmall
                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, attachFlow.width - Theme.itemSizeExtraSmall
                                    - 3 * Theme.paddingMedium)
                    truncationMode: TruncationMode.Fade
                    text: chip.att ? appWindow.kAttachmentPrefix + chip.att.name : ""
                    color: chip.ok ? Theme.highlightColor : Theme.errorColor
                    font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
                }
                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    text: "✕"
                    color: chip.highlighted ? Theme.highlightColor : Theme.secondaryColor
                    font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
                }
            }
            onClicked: appWindow.removeAttachment(chip.attIndex)
        }
    }

    // A file the current model cannot read: say so once under the tiles.
    Label {
        id: attachWarning
        visible: attachStrip.visible
                 && appWindow.attachments.length > chatPage.sendableAttachments
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            bottom: attachStrip.top
        }
        z: 7
        text: qsTr("Marked files can't be read by this model and won't be sent")
        wrapMode: Text.Wrap
        color: Theme.errorColor
        font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
    }


    // ── Attach menu ──────────────────────────────────────────────────────
    // The paperclip opens this panel above the input: camera, gallery,
    // videos (models that read video) and files (models that read PDFs).
    Component {
        id: imagePickerPage
        ImagePickerPage {
            onSelectedContentPropertiesChanged: {
                if (selectedContentProperties && selectedContentProperties.filePath)
                    appWindow.addAttachment(selectedContentProperties.filePath)
            }
        }
    }
    Component {
        id: videoPickerPage
        VideoPickerPage {
            onSelectedContentPropertiesChanged: {
                if (selectedContentProperties && selectedContentProperties.filePath)
                    appWindow.addAttachment(selectedContentProperties.filePath)
            }
        }
    }
    Component {
        id: filePickerPage
        FilePickerPage {
            title: qsTr("Attach a file")
            nameFilters: {
                var f = []
                if (appWindow.canAttachPdf) f.push("*.pdf")
                if (appWindow.canAttachImages) f = f.concat(["*.png", "*.jpg", "*.jpeg", "*.gif", "*.webp"])
                if (appWindow.canAttachVideo) f = f.concat(["*.mp4", "*.mov", "*.webm"])
                return f
            }
            onSelectedContentPropertiesChanged: {
                if (selectedContentProperties && selectedContentProperties.filePath)
                    appWindow.addAttachment(selectedContentProperties.filePath)
            }
        }
    }
    function openAttach(what) {
        attachSheet.open = false
        if (what === "camera")
            pageStack.push(Qt.resolvedUrl("CameraPage.qml"),
                           { appWindow: appWindow, allowVideo: appWindow.canAttachVideo })
        else if (what === "gallery")
            pageStack.push(imagePickerPage)
        else if (what === "videos")
            pageStack.push(videoPickerPage)
        else
            pageStack.push(filePickerPage)
    }

    Rectangle {
        id: attachSheet
        property bool open: false
        anchors {
            left: parent.left
            right: parent.right
            bottom: attachStrip.top
        }
        height: attachRow.height + 2 * Theme.paddingLarge
        // Opaque base so the chat does not show through, tinted with the theme.
        color: "black"
        z: 8
        Rectangle {
            anchors.fill: parent
            color: Theme.rgba(Theme.highlightDimmerColor, 0.85)
        }
        visible: opacity > 0
        opacity: open && appWindow.canAttach ? 1 : 0
        transform: Translate {
            y: attachSheet.open ? 0 : Theme.paddingLarge
            Behavior on y { NumberAnimation { duration: 140; easing.type: Easing.OutQuad } }
        }
        Behavior on opacity { FadeAnimation { duration: 140 } }
        // A model without file input (or another session) closes it.
        Connections {
            target: appWindow
            onCurrentSessionIdChanged: attachSheet.open = false
            onCanAttachChanged: if (!appWindow.canAttach) attachSheet.open = false
        }

        Row {
            id: attachRow
            anchors.centerIn: parent
            spacing: Theme.paddingLarge * 1.5
            Repeater {
                model: {
                    var m = [{ id: "camera", icon: "image://theme/icon-m-camera", text: qsTr("Camera") },
                             { id: "gallery", icon: "image://theme/icon-m-image", text: qsTr("Gallery") }]
                    if (appWindow.canAttachVideo)
                        m.push({ id: "videos", icon: "image://theme/icon-m-video", text: qsTr("Videos") })
                    if (appWindow.canAttachPdf)
                        m.push({ id: "files", icon: "image://theme/icon-m-document", text: qsTr("Files") })
                    return m
                }
                BackgroundItem {
                    id: tile
                    width: Theme.itemSizeLarge
                    height: tileCol.height
                    onClicked: chatPage.openAttach(modelData.id)
                    Column {
                        id: tileCol
                        width: parent.width
                        spacing: Theme.paddingSmall
                        Rectangle {
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: Theme.itemSizeMedium
                            height: width
                            radius: width / 2
                            color: tile.highlighted ? Theme.rgba(Theme.highlightBackgroundColor, 0.5)
                                                    : Theme.rgba(Theme.primaryColor, 0.12)
                            Icon {
                                anchors.centerIn: parent
                                source: modelData.icon
                                highlighted: tile.highlighted
                            }
                        }
                        Label {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            text: modelData.text
                            truncationMode: TruncationMode.Fade
                            font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
                            color: tile.highlighted ? Theme.highlightColor : Theme.primaryColor
                        }
                    }
                }
            }
        }
    }

    // ── Revert banner ─────────────────────────────────────────────────────
    // Shown while the session tail is reverted (worker control line
    // `[[reverted]]1`). "Restore" calls unrevert and the full history reloads.
    Item {
        id: revertStrip
        readonly property bool on: appWindow.reverted === true
        visible: on
        height: visible ? Theme.itemSizeSmall : 0
        anchors {
            left: parent.left
            right: parent.right
            bottom: attachStrip.top
        }

        Rectangle {
            anchors.fill: parent
            color: Theme.rgba(Theme.highlightColor, 0.15)
        }

        Label {
            anchors {
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            text: qsTr("History reverted")
            font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
            color: Theme.secondaryColor
        }

        Button {
            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            height: Theme.itemSizeExtraSmall
            text: qsTr("Restore")
            onClicked: appWindow.unrevertSession()
        }

        // Hairline separating the banner from the chat above.
        Rectangle {
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
            }
            height: 1
            color: Theme.rgba(Theme.primaryColor, 0.15)
        }
    }

    Item {
        id: inputPanel
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        // One row of buttons; the text field grows with its text (up to
        // about five lines) and the buttons stay on the bottom line.
        height: Math.max(chatPage.barButtonSize, input.visible ? input.height : 0) + Theme.paddingMedium
        Behavior on height { NumberAnimation { duration: 100; easing.type: Easing.OutQuad } }
        readonly property int maxInputHeight: Theme.itemSizeMedium
                                              + 4 * Math.round(input.font.pixelSize * 1.3)

        // Rounded field behind the text input.
        Rectangle {
            visible: input.visible
            x: input.x
            width: input.width
            height: Math.max(chatPage.barButtonSize - Theme.paddingMedium, input.height)
            // Centred on the text: the field has a little more margin below
            // its text than above.
            y: input.y + (input.height - height) / 2
               - (Theme.paddingMedium - Theme.paddingSmall) / 2
            radius: Math.min(height / 2, chatPage.barButtonSize / 2)
            color: Theme.rgba(Theme.primaryColor, input.activeFocus ? 0.10 : 0.06)
            border.width: input.activeFocus ? 1 : 0
            border.color: Theme.rgba(Theme.highlightColor, 0.4)
        }

        // ── Fixed input panel layouts ──────────────────────────────────────
        // There are few variants — we define each explicitly via State (anchors), without
        // computing coordinates (laying out with "floating" X used to break).
        //  - text:      [input, send]
        //  - ptt left:  [mic, input, send]
        //  - ptt center:[input, mic, send]
        //  - ptt right: [input, mic, send]  (mic next to the send button)
        //  - voice left/center/right: [mic, send] / [mic·center, send] / [mic, send]
        property string layoutKey: {
            if (appWindow.inputMode === "voice")
                return "voice" + appWindow.pttPosition
            if (appWindow.inputMode === "text")
                return "text"
            // In "text + dictation" mode the center is not allowed (only left/right).
            if (appWindow.pttPosition === "center")
                return "pttleft"
            return "ptt" + appWindow.pttPosition
        }
        states: [
            State {
                name: "text"
                when: inputPanel.layoutKey === "text"
                AnchorChanges {
                    target: input
                    anchors.left: inputPanel.left
                    anchors.right: send.left
                }
                PropertyChanges { target: input; anchors.leftMargin: Theme.paddingSmall }
                PropertyChanges { target: input; anchors.rightMargin: Theme.paddingSmall }
            },
            State {
                name: "pttleft"
                when: inputPanel.layoutKey === "pttleft"
                AnchorChanges {
                    target: mic
                    anchors.left: inputPanel.left
                }
                AnchorChanges {
                    target: input
                    anchors.left: mic.right
                    anchors.right: send.left
                }
                PropertyChanges { target: mic; visible: true }
                PropertyChanges { target: mic; anchors.leftMargin: Theme.paddingSmall }
                PropertyChanges { target: input; anchors.leftMargin: Theme.paddingSmall }
                PropertyChanges { target: input; anchors.rightMargin: Theme.paddingSmall }
            },
            State {
                name: "pttcenter"
                when: inputPanel.layoutKey === "pttcenter"
                AnchorChanges {
                    target: mic
                    anchors.horizontalCenter: inputPanel.horizontalCenter
                }
                AnchorChanges {
                    target: input
                    anchors.left: inputPanel.left
                    anchors.right: mic.left
                }
                PropertyChanges { target: mic; visible: true }
                PropertyChanges { target: input; anchors.leftMargin: Theme.paddingSmall }
                PropertyChanges { target: input; anchors.rightMargin: Theme.paddingSmall }
            },
            State {
                name: "pttright"
                when: inputPanel.layoutKey === "pttright"
                AnchorChanges {
                    target: mic
                    anchors.right: send.left
                }
                AnchorChanges {
                    target: input
                    anchors.left: inputPanel.left
                    anchors.right: mic.left
                }
                PropertyChanges { target: mic; visible: true }
                PropertyChanges { target: mic; anchors.rightMargin: Theme.paddingSmall }
                PropertyChanges { target: input; anchors.leftMargin: Theme.paddingSmall }
                PropertyChanges { target: input; anchors.rightMargin: Theme.paddingSmall }
            },
            State {
                name: "voiceleft"
                when: inputPanel.layoutKey === "voiceleft"
                AnchorChanges {
                    target: mic
                    anchors.left: inputPanel.left
                }
                PropertyChanges { target: mic; visible: true }
                PropertyChanges { target: mic; anchors.leftMargin: Theme.paddingSmall }
            },
            State {
                name: "voicecenter"
                when: inputPanel.layoutKey === "voicecenter"
                AnchorChanges {
                    target: mic
                    anchors.horizontalCenter: inputPanel.horizontalCenter
                }
                PropertyChanges { target: mic; visible: true }
            },
            State {
                name: "voiceright"
                when: inputPanel.layoutKey === "voiceright"
                AnchorChanges {
                    target: mic
                    anchors.right: send.left
                }
                PropertyChanges { target: mic; visible: true }
                PropertyChanges { target: mic; anchors.rightMargin: Theme.paddingSmall }
            }
        ]

        IconButton {
            id: mic
            visible: false
            width: chatPage.barButtonSize
            height: chatPage.barButtonSize
            icon.width: chatPage.barIconSize
            icon.height: chatPage.barIconSize
            icon.sourceSize.width: chatPage.barIconSize
            icon.sourceSize.height: chatPage.barIconSize
            anchors.bottom: inputPanel.bottom
            anchors.bottomMargin: Theme.paddingMedium / 2
            // The horizontal anchor is set only by state (left/center/right),
            // so that no anchor conflict arises when centering.
            icon.source: (appWindow.recording || appWindow.recognizing)
                          ? "" : "image://theme/icon-m-mic"

            // A loader instead of the icon while recording or recognising.
            // Centered exactly on the button (same as the mic icon it replaces);
            // `anchors.fill` used to offset it away from the icon's spot.
            BusyIndicator {
                anchors.centerIn: parent
                size: BusyIndicatorSize.Medium
                running: appWindow.recording || appWindow.recognizing
                visible: running
            }

            // PTT hold: pressed → recording, released → stop + recognition.
            MouseArea {
                anchors.fill: parent
                onPressed: appWindow.startPttHold()
                onReleased: appWindow.stopPttHold()
                onCanceled: appWindow.stopPttHold()
            }
        }

        IconButton {
            id: send
            // During a retry the agent is "busy" but nothing is generating:
            // show the (disabled) send button instead of an unusable stop.
            visible: !chatPage.busy || chatPage.retryInfo !== null
            width: chatPage.barButtonSize
            height: chatPage.barButtonSize
            icon.width: chatPage.barIconSize
            icon.height: chatPage.barIconSize
            icon.sourceSize.width: chatPage.barIconSize
            icon.sourceSize.height: chatPage.barIconSize
            anchors {
                right: inputPanel.right
                rightMargin: Theme.paddingSmall
                bottom: inputPanel.bottom
                bottomMargin: Theme.paddingMedium / 2
            }
            icon.source: "image://theme/icon-m-enter-accept"
            // A pending retry is "busy" for the agent but not for the user: the
            // button stays visible (disabled) so it does not turn into a useless
            // stop button, and the composer keeps accepting text (see `input`).
            enabled: (input.text.length > 0 || chatPage.sendableAttachments > 0)
                     && header.retry === null
            onClicked: {
                appWindow.send(input.text)
                input.text = ""
            }
        }

        IconButton {
            id: stop
            visible: chatPage.busy && header.retry === null
            width: chatPage.barButtonSize
            height: chatPage.barButtonSize
            icon.width: chatPage.barIconSize
            icon.height: chatPage.barIconSize
            icon.sourceSize.width: chatPage.barIconSize
            icon.sourceSize.height: chatPage.barIconSize
            anchors {
                right: inputPanel.right
                rightMargin: Theme.paddingSmall
                bottom: inputPanel.bottom
                bottomMargin: Theme.paddingMedium / 2
            }
            icon.source: "image://theme/icon-m-stop"
            onClicked: appWindow.stopAgent()
        }

        TextArea {
            id: input
            visible: appWindow.inputMode !== "voice"
            // Typing stays possible while the server retries: only sending is
            // blocked. Previously a broken binding (`statusHeader` is the
            // component's internal id, not visible here) silently left this
            // `false` forever, which locked the user out of the composer.
            enabled: !chatPage.busy || chatPage.retryInfo !== null
            background: null
            font.pixelSize: appWindow.fs(Theme.fontSizeMedium)
            // We draw the placeholder inside the field, not as a layer on top — otherwise
            // the field looks two-level.
            labelVisible: false
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignLeft
            // Natural height, growing with the text; while shorter than the
            // button row it sits centred on it.
            height: Math.min(implicitHeight, inputPanel.maxInputHeight)
            anchors.bottom: inputPanel.bottom
            anchors.bottomMargin: Theme.paddingMedium / 2
                                  + Math.max(0, (chatPage.barButtonSize - height) / 2)
            // Default anchors (width in the initial layout [input, send]);
            // the state overrides them for the chosen scheme.
            anchors.left: inputPanel.left
            anchors.leftMargin: Theme.paddingSmall
            anchors.right: send.left
            anchors.rightMargin: Theme.paddingSmall
            placeholderText: chatPage.busy
                             ? qsTr("Agent is working…")
                             : qsTr("Agent prompt…")
            // Paperclip inside the field, for models that read images/PDFs.
            rightItem: appWindow.canAttach ? attachButton : null
            EnterKey.enabled: text.length > 0 && !chatPage.busy
            EnterKey.iconSource: "image://theme/icon-m-enter-accept"
            EnterKey.onClicked: {
                appWindow.send(input.text)
                input.text = ""
            }
        }

        IconButton {
            id: attachButton
            visible: appWindow.canAttach && input.visible
            width: chatPage.barIconSize
            height: chatPage.barIconSize
            icon.source: "image://theme/icon-m-attach"
            icon.width: chatPage.barIconSize
            icon.height: chatPage.barIconSize
            icon.sourceSize.width: chatPage.barIconSize
            icon.sourceSize.height: chatPage.barIconSize
            enabled: !chatPage.busy || chatPage.retryInfo !== null
            highlighted: down || attachSheet.open
            onClicked: attachSheet.open = !attachSheet.open
        }
    }
}
