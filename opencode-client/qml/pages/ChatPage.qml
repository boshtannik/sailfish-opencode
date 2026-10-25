import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"

Item {
    id: chatPage

    property var appWindow

    readonly property bool busy: appWindow.statusBase(appWindow.statusText) === "busy"

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

    // Compaction activity banner. While the shown session is compacted (manual
    // "Compact" or auto on context overflow) the summary streams as a normal
    // growing bubble; this banner marks the operation (spinner + label) until
    // `session.compacted` reloads the feed.
    Rectangle {
        id: compactionBanner
        anchors {
            top: header.bottom
            left: parent.left
            right: parent.right
        }
        height: visible ? Theme.itemSizeSmall : 0
        visible: chatPage.appWindow ? chatPage.appWindow.compactionActive : false
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
            font.pixelSize: Theme.fontSizeSmall
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
            top: compactionBanner.bottom
            left: parent.left
            right: parent.right
            bottom: retryStrip.visible ? retryStrip.top : (revertStrip.visible ? revertStrip.top : inputPanel.top)
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
        onCountChanged: if (stickBottom) positionViewAtEnd()
        onContentHeightChanged: if (stickBottom) positionViewAtEnd()
        onMovementEnded: stickBottom = atYEnd
        onFlickEnded: stickBottom = atYEnd
        VerticalScrollDecorator {}
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
            property bool menuOpen: userMenu._open
            height: visible ? (line.isTool
                               ? toolRow.height + Theme.paddingSmall
                               : bubble.height + Theme.paddingSmall
                                 + (line.menuOpen ? userMenu.height : 0)) : 0

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
            property var segments: line.isTool ? [] : chatPage.splitBody(line.body)
            // Server messageID of this bubble (user lines only): enables the
            // long-press revert/fork menu. Empty for agent/system bubbles.
            property string msgId: appWindow.msgIdOf(line.raw)
            readonly property real maxW: chatList.width - 2 * Theme.horizontalPageMargin

            Rectangle {
                id: bubble
                visible: !line.isTool
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
                height: bodyCol.height
                        + (footRow.height > 0 ? footRow.height + Theme.paddingSmall : 0)
                        + 2 * Theme.paddingSmall
                anchors.right: line.isUser ? parent.right : undefined
                anchors.left: line.isUser ? undefined : parent.left
                anchors.rightMargin: line.isUser ? Theme.horizontalPageMargin : 0
                anchors.leftMargin: line.isUser ? 0 : Theme.horizontalPageMargin

                Column {
                    id: bodyCol
                    x: Theme.paddingSmall
                    y: Theme.paddingSmall
                    width: bubble.width - 2 * Theme.paddingSmall
                    spacing: Theme.paddingSmall

                    Repeater {
                        model: line.segments
                        delegate: Item {
                            id: segItem
                            width: bodyCol.width
                            property var seg: modelData
                            height: segItem.seg.code ? segBox.height
                                    : segItem.seg.link ? linkRow.height
                                                       : proseLbl.height

                            // Free-form prose (markdown → rich text).
                            Label {
                                id: proseLbl
                                visible: !segItem.seg.code && !segItem.seg.link
                                width: parent.width
                                text: chatPage.mdBlock(chatPage.mdEscape(segItem.seg.text))
                                textFormat: Text.RichText
                                wrapMode: Text.Wrap
                                onLinkActivated: Qt.openUrlExternally(link)
                                font.pixelSize: Theme.fontSizeSmall
                                font.italic: line.isThink
                                color: (line.isThink || line.isSvc)
                                       ? Theme.secondaryColor : Theme.primaryColor
                            }

                            // A whole-line link: the URL itself + a copy button
                            // right beside it.
                            // Whole-line link: label + copy button. The button is pinned to the row's
                            // right edge and the label is cut off at the icon's edge —
                            // the icon can never sit ON the text.
                            Item {
                                id: linkRow
                                visible: segItem.seg.link
                                width: parent.width
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
                                    font.pixelSize: Theme.fontSizeSmall
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
                                    width: Theme.itemSizeSmall
                                    height: Theme.itemSizeSmall
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    icon.source: "image://theme/icon-m-clipboard"
                                    onClicked: appWindow.copyText(segItem.seg.url)
                                }
                            }

                            // Fenced code block: own card with a light header bar
                            // and a copy button in its top-right corner.
                            Rectangle {
                                id: segBox
                                visible: segItem.seg.code
                                width: parent.width
                                radius: Theme.paddingSmall
                                color: Theme.rgba(Theme.highlightColor, 0.08)
                                height: codeHeader.height
                                        + codeLbl.height + 2 * Theme.paddingSmall
                                Rectangle {
                                    id: codeHeader
                                    width: parent.width
                                    height: Theme.itemSizeSmall
                                    radius: Theme.paddingSmall
                                    color: Theme.rgba(Theme.highlightColor, 0.15)
                                    Label {
                                        anchors.left: parent.left
                                        anchors.leftMargin: Theme.paddingSmall
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: segItem.seg.lang
                                        visible: segItem.seg.lang.length > 0
                                        color: Theme.secondaryColor
                                        font.pixelSize: Theme.fontSizeExtraSmall
                                    }
                                    IconButton {
                                        id: copyCodeBtn
                                        anchors.right: parent.right
                                        anchors.rightMargin: Theme.paddingSmall
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: Theme.itemSizeSmall
                                        height: Theme.itemSizeSmall
                                        icon.source: "image://theme/icon-m-clipboard"
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
                                        font.pixelSize: Theme.fontSizeSmall
                                        color: Theme.primaryColor
                                    }
                                    HorizontalScrollDecorator {}
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
                    y: bodyCol.y + bodyCol.height + Theme.paddingSmall
                    spacing: Theme.paddingMedium

                    // 📋 copy this bubble to the clipboard — the agent's answer
                    // (or thinking) and the user's own messages alike.
                    IconButton {
                        id: copyBtn
                        visible: !line.isSystem
                        width: visible ? Theme.itemSizeMedium : 0
                        height: visible ? Theme.itemSizeMedium : 0
                        icon.source: "image://theme/icon-m-clipboard"
                        onClicked: appWindow.copyText(line.body)
                    }

                    // 🔊 speak this bubble: agent answers only (not user).
                    // Hit area — the full `iconSizeMedium` icon size, so
                    // it is easy to hit with a finger.
                    // When THIS VERY bubble is being synthesised/spoken, the button
                    // turns into "stop" (stop icon + loader during synthesis),
                    // so TTS can be interrupted right on the bubble itself, not via a separate
                    // button below.
                    IconButton {
                        id: speakBtn
                        visible: !line.isUser && !line.isSystem
                                 && appWindow.ttsModelReady
                                 && appWindow.ttsMode !== "off"
                        width: visible ? Theme.itemSizeMedium : 0
                        height: visible ? Theme.itemSizeMedium : 0
                        property bool active: appWindow.speakingBubble === line.body
                                              && (appWindow.ttsSynth || appWindow.ttsPlaying)
                        icon.source: active
                                     ? "image://theme/icon-m-stop"
                                     : "image://theme/icon-m-speaker-on"
                        highlighted: active
                        // While this bubble is being synthesised — a loader instead of the icon.
                        BusyIndicator {
                            anchors.centerIn: parent
                            running: visible
                            visible: appWindow.ttsSynth
                                     && appWindow.speakingBubble === line.body
                        }
                        onClicked: {
                            if (active)
                                appWindow.stopTts()
                            else
                                appWindow.speakText(line.body)
                        }
                    }

                    Label {
                        id: tsLabel
                        text: chatPage.tsLabel(line.tsMs)
                        anchors.verticalCenter: parent.verticalCenter
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: Theme.secondaryColor
                    }
                }

                // Long-press a user bubble → edit/fork menu (mirror of the TUI).
                // Disabled while the agent is busy: reverting needs an idle session.
                MouseArea {
                    anchors.fill: parent
                    enabled: line.isUser && line.msgId !== "" && !chatPage.busy
                    onPressAndHold: userMenu.open(line)
                }
            }

            // Revert/fork actions for this user message. ContextMenu reparents
            // itself to the delegate item when opened (Silica ContextMenu.open).
            ContextMenu {
                id: userMenu
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
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.secondaryColor
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
            bottom: inputPanel.top
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
            font.pixelSize: Theme.fontSizeExtraSmall
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

    // ── Strip shown when a server retry / quota reset countdown is pending ──
    Item {
        id: retryStrip
        readonly property bool on: statusHeader.retry !== null
        visible: on
        height: visible ? retryCol.height + 2 * Theme.paddingSmall : 0
        anchors {
            left: parent.left
            right: parent.right
            bottom: revertStrip.visible ? revertStrip.top : inputPanel.top
        }
        clip: true

        Rectangle {
            anchors.fill: parent
            color: Theme.rgba("#ff9800", 0.15)
        }

        Column {
            id: retryCol
            anchors {
                left: parent.left
                right: parent.right
                margins: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            spacing: Theme.paddingSmall

            Label {
                width: parent.width
                text: statusHeader.retryText
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: "#ff9800"
            }
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
        // Single input level: the panel height = the height of one row of buttons.
        height: Theme.itemSizeMedium + Theme.paddingMedium

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
            width: Theme.itemSizeMedium
            height: Theme.itemSizeMedium
            anchors.verticalCenter: inputPanel.verticalCenter
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
            visible: !chatPage.busy
            width: Theme.itemSizeMedium
            height: Theme.itemSizeMedium
            anchors {
                right: inputPanel.right
                rightMargin: Theme.paddingSmall
                verticalCenter: inputPanel.verticalCenter
            }
            icon.source: "image://theme/icon-m-enter-accept"
            enabled: input.text.length > 0 && statusHeader.retry === null
            onClicked: {
                appWindow.send(input.text)
                input.text = ""
            }
        }

        IconButton {
            id: stop
            visible: chatPage.busy
            width: Theme.itemSizeMedium
            height: Theme.itemSizeMedium
            anchors {
                right: inputPanel.right
                rightMargin: Theme.paddingSmall
                verticalCenter: inputPanel.verticalCenter
            }
            icon.source: "image://theme/icon-m-stop"
            onClicked: appWindow.stopAgent()
        }

        TextArea {
            id: input
            visible: appWindow.inputMode !== "voice"
            enabled: !chatPage.busy && statusHeader.retry === null
            background: null
            // We draw the placeholder inside the field, not as a layer on top — otherwise
            // the field looks two-level.
            labelVisible: false
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignLeft
            // One line of the same height as the buttons: the bottom aligns to one level.
            height: Theme.itemSizeMedium
            anchors.verticalCenter: inputPanel.verticalCenter
            // Default anchors (width in the initial layout [input, send]);
            // the state overrides them for the chosen scheme.
            anchors.left: inputPanel.left
            anchors.leftMargin: Theme.paddingSmall
            anchors.right: send.left
            anchors.rightMargin: Theme.paddingSmall
            placeholderText: chatPage.busy
                             ? qsTr("Agent is working…")
                             : qsTr("Agent prompt…")
            EnterKey.enabled: text.length > 0 && !chatPage.busy
            EnterKey.iconSource: "image://theme/icon-m-enter-accept"
            EnterKey.onClicked: {
                appWindow.send(input.text)
                input.text = ""
            }
        }
    }
}
