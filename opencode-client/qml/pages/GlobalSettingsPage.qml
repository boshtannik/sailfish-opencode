import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: globalSettingsPage

    // Size of the icons in the copy/speaker switches: follows the Icon size
    // slider while it is dragged.
    readonly property int iconPreviewSize: Math.round(Theme.iconSizeMedium
        * (iconScaleSlider ? iconScaleSlider.sliderValue / 100 : appWindow.iconScale))

    property bool providerKeySet: false
    // Mirrors `auth::ANTHROPIC_PROVIDER` in Rust.
    readonly property string kAnthropicProvider: "anthropic"
    property bool anthropicKeySet: false
    // The saved key was refused by Anthropic (a turn failed with 401).
    property bool anthropicKeyRejected: false
    // OpenCode Console device-login state, mirrored from the root poll().
    readonly property var login: (typeof appWindow !== "undefined" && appWindow
                                  && appWindow.consoleLogin)
                                 ? appWindow.consoleLogin
                                 : ({ "state": "idle", "url": "", "code": "", "message": "" })

    Component.onCompleted: refreshProviderKey()

    function refreshProviderKey() {
        if (typeof appWindow !== "undefined" && appWindow) {
            providerKeySet = appWindow.providerKeySet()
            anthropicKeySet = appWindow.providerKeySetFor(kAnthropicProvider)
            anthropicKeyRejected = appWindow.providerKeyRejectedFor(kAnthropicProvider)
        }
    }

    // "Unlock" from the model picker: bring the key field into view and focus it.
    Connections {
        target: globalSettingsPage.appWindow
        onProviderKeyRequested: {
            if (provider !== globalSettingsPage.kAnthropicProvider)
                return
            settingsFlick.contentY = Math.max(0, Math.min(
                claudeHeader.y + column.y - Theme.paddingLarge,
                settingsFlick.contentHeight - settingsFlick.height))
            anthropicKeyField.forceActiveFocus()
        }
    }

    function saveAnthropicKey() {
        if (typeof appWindow === "undefined" || !appWindow
                || anthropicKeyField.text.trim().length === 0)
            return
        appWindow.saveProviderKeyFor(kAnthropicProvider, anthropicKeyField.text)
        anthropicKeyField.text = ""
        anthropicKeyField.focus = false
    }

    // The worker refreshes the connected-provider cache together with the
    // model catalog (at start-up and after a key is saved), and the root
    // poll re-assigns `models` each round, so re-read the key state then.
    Connections {
        target: globalSettingsPage.appWindow
        onModelsChanged: globalSettingsPage.refreshProviderKey()
    }

    property var appWindow

    // Syncing the PTT position dropdown. The currentIndex binding breaks
    // on manual menu item selection, so we set the index explicitly.
    //
    // The index space is the one of the *menu*: the hidden "Center" item still
    // occupies its slot, so "Right" is index 2 in both modes. Using a shortened
    // list for Text + PTT was the bug — tapping "Right" produced index 2,
    // `available[2]` was undefined and the position silently stayed "left".
    readonly property var pttPositions: ["left", "center", "right"]
    // The centre slot exists only in the dictation-only ("voice") mode.
    readonly property bool pttCenterAllowed: appWindow.inputMode === "voice"

    function syncPttCombo() {
        // Read the mode directly: the `pttCenterAllowed` binding may not have
        // re-evaluated yet when this is called from the mode dropdown handler.
        var centerAllowed = appWindow.inputMode === "voice"
        var idx = pttPositions.indexOf(appWindow.pttPosition)
        if (idx === 1 && !centerAllowed)
            idx = 0
        if (idx < 0)
            idx = 0
        pttPosCombo.currentIndex = idx
    }

    SilicaFlickable {
        id: settingsFlick
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

            SectionHeader { text: qsTr("Appearance") }

            Slider {
                id: fontScaleSlider
                width: parent.width
                label: qsTr("Text size")
                minimumValue: appWindow.fontScaleMin * 100
                maximumValue: appWindow.fontScaleMax * 100
                stepSize: 5
                value: Math.round(appWindow.fontScale * 100)
                valueText: Math.round(sliderValue) + " %"
                // Apply on release: re-laying out the whole feed on every
                // slider step would make dragging stutter.
                onDownChanged: if (!down) appWindow.setFontScale(sliderValue / 100)
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("Preview: the quick brown fox jumps over the lazy dog.")
                wrapMode: Text.Wrap
                font.pixelSize: Math.round(Theme.fontSizeSmall * fontScaleSlider.sliderValue / 100)
                color: Theme.secondaryHighlightColor
            }

            SectionHeader { text: qsTr("Messages") }

            // The switches show the icons they control, at the size picked
            // below, so the slider previews its effect live.
            IconTextSwitch {
                text: qsTr("Copy button on messages")
                icon.source: "image://theme/icon-m-clipboard"
                icon.width: globalSettingsPage.iconPreviewSize
                icon.height: globalSettingsPage.iconPreviewSize
                icon.sourceSize.width: globalSettingsPage.iconPreviewSize
                icon.sourceSize.height: globalSettingsPage.iconPreviewSize
                checked: appWindow.showCopyButton
                onCheckedChanged: appWindow.setShowCopyButton(checked)
            }

            IconTextSwitch {
                text: qsTr("Speaker button on answers")
                description: qsTr("Read an answer aloud (needs a speech synthesis model)")
                icon.source: "image://theme/icon-m-speaker-on"
                icon.width: globalSettingsPage.iconPreviewSize
                icon.height: globalSettingsPage.iconPreviewSize
                icon.sourceSize.width: globalSettingsPage.iconPreviewSize
                icon.sourceSize.height: globalSettingsPage.iconPreviewSize
                checked: appWindow.showSpeakButton
                onCheckedChanged: appWindow.setShowSpeakButton(checked)
            }

            Slider {
                id: iconScaleSlider
                width: parent.width
                label: qsTr("Icon size")
                minimumValue: appWindow.iconScaleMin * 100
                maximumValue: appWindow.iconScaleMax * 100
                stepSize: 5
                value: Math.round(appWindow.iconScale * 100)
                valueText: Math.round(sliderValue) + " %"
                // The icons above follow the slider live; the chat applies
                // the size on release.
                onDownChanged: if (!down) appWindow.setIconScale(sliderValue / 100)
            }

            // The input bar icons at the chosen size (mic, paperclip, send).
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: Theme.paddingLarge * 2
                Repeater {
                    model: ["image://theme/icon-m-mic", "image://theme/icon-m-attach",
                            "image://theme/icon-m-enter-accept"]
                    Icon {
                        source: modelData
                        width: globalSettingsPage.iconPreviewSize
                        height: globalSettingsPage.iconPreviewSize
                        sourceSize.width: globalSettingsPage.iconPreviewSize
                        sourceSize.height: globalSettingsPage.iconPreviewSize
                    }
                }
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("Message buttons and the input bar (mic, attach, send)")
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
            }

            TextSwitch {
                text: qsTr("Label thinking and answers")
                description: qsTr("Show \"Answer\" above the model's replies; thinking is always marked")
                checked: appWindow.labelRoles
                onCheckedChanged: appWindow.setLabelRoles(checked)
            }

            SectionHeader { text: qsTr("Thinking") }

            TextSwitch {
                text: qsTr("Show model thinking (reasoning)")
                description: qsTr("Show the model's reasoning as it works, not just final answers")
                checked: appWindow.showReasoning
                onCheckedChanged: appWindow.setShowReasoning(checked)
            }

            TextSwitch {
                text: qsTr("Expand thinking")
                description: qsTr("Show the whole reasoning instead of its last lines (tap \"Thinking\" on a message to toggle it)")
                checked: appWindow.expandThinking
                onCheckedChanged: appWindow.setExpandThinking(checked)
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
                currentIndex: 0
                menu: ContextMenu {
                    MenuItem { text: qsTr("Left") }
                    MenuItem {
                        text: qsTr("Center")
                        // In Text + PTT the centre is not allowed — hide the item.
                        // A hidden MenuItem still keeps its index (see above).
                        visible: globalSettingsPage.pttCenterAllowed
                    }
                    MenuItem { text: qsTr("Right") }
                }
                onCurrentIndexChanged: {
                    var v = globalSettingsPage.pttPositions
                    if (currentIndex < 0 || currentIndex >= v.length)
                        return
                    var pos = v[currentIndex]
                    // Defensive: the hidden slot cannot be activated, but never
                    // let a stale index push the button into the centre.
                    if (pos === "center" && appWindow.inputMode !== "voice") {
                        globalSettingsPage.syncPttCombo()
                        return
                    }
                    appWindow.pttPosition = pos
                }
                Component.onCompleted: globalSettingsPage.syncPttCombo()
            }

            TextSwitch {
                text: qsTr("Vibrate on PTT")
                description: qsTr("Vibrate when pressing and releasing the PTT button")
                checked: appWindow.pttVibrate
                onCheckedChanged: appWindow.pttVibrate = checked
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

            // Both the icon legend and the "which model is in use" info live on
            // the Voice models page itself: the downloaded models there are
            // already split into "Downloaded · Recognition" and
            // "Downloaded · Synthesis" sections, with the active model marked ✓.

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
                font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
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

            SectionHeader { text: qsTr("OpenCode account") }

            Label {
                visible: globalSettingsPage.login.state !== "waiting"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("Sign in to your opencode.ai account. The app shows a code to enter in the browser.")
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
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
                font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
            }

            Label {
                visible: globalSettingsPage.login.state === "waiting"
                anchors.horizontalCenter: parent.horizontalCenter
                text: globalSettingsPage.login.code
                font.pixelSize: appWindow.fs(Theme.fontSizeExtraLarge)
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
                    font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
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
                font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
            }

            SectionHeader { id: claudeHeader; text: qsTr("Claude (Anthropic)") }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: globalSettingsPage.anthropicKeyRejected
                      ? qsTr("Anthropic rejected the saved API key, so Claude models are locked. Enter a valid key to unlock them.")
                      : (globalSettingsPage.anthropicKeySet
                         ? qsTr("API key saved ✓ Pick a Claude model from the model name at the top of the chat. Enter a new key to replace it.")
                         : qsTr("Add your Anthropic API key to unlock Claude models. Free OpenCode models work without a key."))
                wrapMode: Text.Wrap
                color: globalSettingsPage.anthropicKeyRejected
                       ? "#ff8a80"
                       : (globalSettingsPage.anthropicKeySet ? Theme.highlightColor : Theme.secondaryColor)
                font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
            }

            // A plain field, not PasswordField: a password-type field makes
            // keyboards with a password manager (FUTO Keyboard) pop a "save this
            // login" prompt that asks for the device security code when the
            // field loses focus. ImhSensitiveData keeps the key out of the
            // keyboard's learning/suggestions; the field is cleared on save.
            TextField {
                id: anthropicKeyField
                width: parent.width
                label: qsTr("Anthropic API key")
                placeholderText: qsTr("Anthropic API key (sk-ant-…)")
                inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText
                                  | Qt.ImhNoAutoUppercase | Qt.ImhPreferLatin
                EnterKey.enabled: text.trim().length > 0
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: globalSettingsPage.saveAnthropicKey()
            }

            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: Theme.paddingMedium
                Button {
                    text: qsTr("Save key")
                    enabled: anthropicKeyField.text.trim().length > 0
                    onClicked: globalSettingsPage.saveAnthropicKey()
                }
                Button {
                    text: qsTr("Get a key")
                    onClicked: Qt.openUrlExternally(
                                   globalSettingsPage.appWindow.anthropicKeysUrl())
                }
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
                font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
                wrapMode: Text.Wrap
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
