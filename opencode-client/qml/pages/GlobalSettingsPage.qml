import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: globalSettingsPage

    property bool providerKeySet: false
    // OpenCode Console device-login state, mirrored from the root poll().
    readonly property var login: (typeof appWindow !== "undefined" && appWindow
                                  && appWindow.consoleLogin)
                                 ? appWindow.consoleLogin
                                 : ({ "state": "idle", "url": "", "code": "", "message": "" })

    Component.onCompleted: refreshProviderKey()

    function refreshProviderKey() {
        if (typeof appWindow !== "undefined" && appWindow)
            providerKeySet = appWindow.providerKeySet()
    }

    function saveProviderKey() {
        if (typeof appWindow === "undefined" || !appWindow || apiKeyField.text.length === 0)
            return
        appWindow.saveProviderKey(apiKeyField.text)
        apiKeyField.text = ""
        keyRefreshTimer.restart()
    }

    property var appWindow

    // Syncing the PTT position dropdown. The currentIndex binding breaks
    // on manual menu item selection, so we set the index explicitly.
    readonly property var pttPositions: ["left", "center", "right"]
    function syncPttCombo() {
        var v = pttPosCombo.available
        var idx = v.indexOf(appWindow.pttPosition)
        if (idx < 0)
            idx = 0
        pttPosCombo.currentIndex = idx
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: pageHeader.height + column.height + Theme.paddingLarge

        PageHeader {
            id: pageHeader
            title: qsTr("Settings")
        }

        Column {
            id: column
            anchors {
                top: pageHeader.bottom
                left: parent.left
                right: parent.right
            }

            SectionHeader { text: qsTr("Input") }

            ComboBox {
                label: qsTr("Mode")
                currentIndex: ["text", "text_ptt", "voice"].indexOf(appWindow.inputMode)
                menu: ContextMenu {
                    MenuItem { text: qsTr("Text only") }
                    MenuItem { text: qsTr("Text + PTT") }
                    MenuItem { text: qsTr("Voice") }
                }
                onCurrentIndexChanged: {
                    var v = ["text", "text_ptt", "voice"]
                    if (currentIndex >= 0 && currentIndex < v.length) {
                        appWindow.inputMode = v[currentIndex]
                        // For text_ptt the dictation button can only be on the
                        // left or right of the input — the center is not allowed.
                        if (v[currentIndex] === "text_ptt"
                            && appWindow.pttPosition === "center")
                            appWindow.pttPosition = "left"
                        // Voice-only: the dictation button defaults to the center
                        // (unless the user has already placed it elsewhere).
                        if (v[currentIndex] === "voice"
                            && appWindow.pttPosition === "left")
                            appWindow.pttPosition = "center"
                        // The PTT position dropdown recomputes the available items.
                        globalSettingsPage.syncPttCombo()
                    }
                }
            }

            ComboBox {
                id: pttPosCombo
                label: qsTr("PTT button position")
                // Center is available only in "Dictation only" mode.
                property var available: appWindow.inputMode === "voice"
                                        ? globalSettingsPage.pttPositions
                                        : ["left", "right"]
                currentIndex: 0
                menu: ContextMenu {
                    MenuItem { text: qsTr("Left") }
                    MenuItem {
                        text: qsTr("Center")
                        // In text_ptt the center is not allowed — hide the item.
                        visible: appWindow.inputMode === "voice"
                    }
                    MenuItem { text: qsTr("Right") }
                }
                onCurrentIndexChanged: {
                    var v = pttPosCombo.available
                    if (currentIndex >= 0 && currentIndex < v.length)
                        appWindow.pttPosition = v[currentIndex]
                }
                Component.onCompleted: globalSettingsPage.syncPttCombo()
            }

            SectionHeader { text: qsTr("Voice") }

            BackgroundItem {
                width: parent.width
                contentHeight: Theme.itemSizeMedium
                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Voice models")
                    truncationMode: TruncationMode.Fade
                    color: parent.highlighted
                           ? Theme.highlightColor : Theme.primaryColor
                }
                onClicked: globalSettingsPage.appWindow.pageStack.push(
                               voiceModelsComponent,
                               { appWindow: globalSettingsPage.appWindow })
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("Recognition:\n%1\nSynthesiser:\n%2")
                      .arg(appWindow.sttModelName())
                      .arg(appWindow.ttsModelName())
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("↓ download\n→ start using\n✓ selected\n✕ cancel\nlong press — remove")
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }

            // — Dictation: a separate section about sending recognized text.
            SectionHeader { text: qsTr("Dictation") }

            TextSwitch {
                text: qsTr("Send immediately")
                checked: appWindow.sendImmediately
                onCheckedChanged: appWindow.sendImmediately = checked
            }

            // — Auto-TTS of answers - a separate section. Available only when
            // a speech synthesis model is downloaded and selected; otherwise — a hint.
            SectionHeader { text: qsTr("Voice answers") }

            Label {
                visible: !appWindow.ttsModelReady
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("Download and select a speech synthesis model to enable\nvoice answers (Settings → Voice models).")
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }

            ComboBox {
                label: qsTr("Free hands / autoplay")
                enabled: appWindow.ttsModelReady
                currentIndex: ["auto", "button", "off"].indexOf(appWindow.ttsMode)
                menu: ContextMenu {
                    MenuItem { text: qsTr("Auto") }
                    MenuItem { text: qsTr("Button") }
                    MenuItem { text: qsTr("Off") }
                }
                onCurrentIndexChanged: {
                    var v = ["auto", "button", "off"]
                    if (currentIndex >= 0 && currentIndex < v.length)
                        appWindow.ttsMode = v[currentIndex]
                }
            }

            SectionHeader { text: qsTr("OpenCode account") }

            Label {
                visible: globalSettingsPage.login.state !== "waiting"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("Sign in to your opencode.ai account. The app shows a code to enter in the browser.")
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }

            Button {
                visible: globalSettingsPage.login.state !== "waiting"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Sign in with OpenCode")
                onClicked: globalSettingsPage.appWindow.startConsoleLogin()
            }

            Label {
                visible: globalSettingsPage.login.state === "waiting"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("Enter this code on the opened page:")
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }

            Label {
                visible: globalSettingsPage.login.state === "waiting"
                anchors.horizontalCenter: parent.horizontalCenter
                text: globalSettingsPage.login.code
                font.pixelSize: Theme.fontSizeExtraLarge
                font.bold: true
                color: Theme.highlightColor
            }

            Button {
                visible: globalSettingsPage.login.state === "waiting"
                        && globalSettingsPage.login.url !== ""
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Open browser again")
                onClicked: Qt.openUrlExternally(globalSettingsPage.login.url)
            }

            Row {
                visible: globalSettingsPage.login.state === "waiting"
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: Theme.paddingMedium
                BusyIndicator {
                    size: BusyIndicatorSize.Small
                    running: parent.visible
                }
                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Waiting for authorization…")
                    color: Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                }
            }

            Button {
                visible: globalSettingsPage.login.state === "waiting"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Cancel")
                onClicked: globalSettingsPage.appWindow.cancelConsoleLogin()
            }

            Label {
                visible: globalSettingsPage.login.state === "success"
                         || globalSettingsPage.login.state === "error"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: globalSettingsPage.login.state === "success"
                      ? qsTr("Signed in ✓")
                      : qsTr("Sign-in failed") + (globalSettingsPage.login.message !== ""
                            ? ": " + globalSettingsPage.login.message : "")
                wrapMode: Text.Wrap
                color: globalSettingsPage.login.state === "success"
                       ? Theme.highlightColor : Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }

            SectionHeader { text: qsTr("General") }

            TextField {
                id: workdirField
                width: parent.width
                text: (typeof appWindow.workdir !== "undefined") ? appWindow.workdir : ""
                placeholderText: qsTr("e.g. /home/defaultuser/opencode-home")
                label: qsTr("Working directory (restart required)")
                onActiveFocusChanged: {
                    if (!activeFocus && typeof appWindow !== "undefined" && appWindow) {
                        appWindow.workdir = text
                        appWindow.setWorkdir(text)
                    }
                }
                EnterKey.enabled: text.length > 0
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: {
                    if (typeof appWindow !== "undefined" && appWindow) {
                        appWindow.workdir = text
                        appWindow.setWorkdir(text)
                    }
                    focus = false
                }
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("Changes take effect after restarting the app")
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
                wrapMode: Text.Wrap
            }

            ComboBox {
                id: apiVersionCombo
                label: qsTr("OpenCode API version")
                description: qsTr("v1 — opencode 1.x, v2 — opencode 2.x. Applied immediately.")
                currentIndex: (appWindow.apiVersion === "v2") ? 1 : 0
                menu: ContextMenu {
                    MenuItem { text: qsTr("v1 (opencode 1.x)") }
                    MenuItem { text: qsTr("v2 (opencode 2.x)") }
                }
                // Persist only real user picks. `currentIndex` is also assigned
                // programmatically (binding + first init), which would otherwise
                // write the value back to settings on every page open and can
                // pin the setting to a stale value (e.g. v1 on a v2-default
                // install). `armed` flips true one event-loop turn after the
                // initial binding settles.
                property bool armed: false
                Component.onCompleted: Qt.callLater(function() { apiVersionCombo.armed = true })
                onCurrentIndexChanged: {
                    if (!armed)
                        return
                    var v = (currentIndex === 1) ? "v2" : "v1"
                    if (typeof appWindow !== "undefined" && appWindow
                        && appWindow.apiVersion !== v)
                        appWindow.setApiVersion(v)
                }
            }

            TextSwitch {
                text: qsTr("Show model thinking (reasoning)")
                description: qsTr("Show the model's reasoning as it works, not just final answers")
                checked: appWindow.showReasoning
                onCheckedChanged: appWindow.setShowReasoning(checked)
            }

            SectionHeader { text: qsTr("Notifications") }

            TextSwitch {
                text: qsTr("Sound when agent finishes (ding)")
                checked: appWindow.soundOnFinish
                onCheckedChanged: appWindow.soundOnFinish = checked
            }

            TextSwitch {
                text: qsTr("Sound on permission request")
                checked: appWindow.soundOnPermission
                onCheckedChanged: appWindow.soundOnPermission = checked
            }

            BackgroundItem {
                width: parent.width
                contentHeight: Theme.itemSizeSmall
                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Sound") + ": " + globalSettingsPage.appWindow.dingSoundName()
                    truncationMode: TruncationMode.Fade
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                onClicked: globalSettingsPage.appWindow.pageStack.push(
                               soundPickerComponent,
                               { appWindow: globalSettingsPage.appWindow })
            }

            BackgroundItem {
                width: parent.width
                contentHeight: Theme.itemSizeSmall
                Label {
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Test notification")
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                onClicked: appWindow.publishNotification(qsTr("opencode"),
                                                         qsTr("Test notification"))
            }

            // The UI language is the system one; translations are loaded natively (.qm).
        }

        VerticalScrollDecorator {}
    }

    Component {
        id: soundPickerComponent
        Page {
            id: soundPickerPage
            property var appWindow

            SilicaListView {
                anchors.fill: parent
                model: soundPickerPage.appWindow !== undefined
                       ? soundPickerPage.appWindow.sounds
                       : []
                header: PageHeader { title: qsTr("Notification sound") }
                delegate: ListItem {
                    contentHeight: Theme.itemSizeSmall
                    onClicked: {
                        soundPickerPage.appWindow.previewDing(modelData.path)
                    }
                    Label {
                        x: Theme.horizontalPageMargin
                        width: parent.width - 2 * Theme.horizontalPageMargin
                                 - Theme.itemSizeSmall
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.name
                        truncationMode: TruncationMode.Fade
                        color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                    }
                    Label {
                        anchors.right: parent.right
                        anchors.rightMargin: Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.path === soundPickerPage.appWindow.dingSound ? "✓" : ""
                        color: Theme.highlightColor
                    }
                }
                VerticalScrollDecorator {}
            }
        }
    }

    Component {
        id: voiceModelsComponent
        VoiceModelsPage { }
    }
}
