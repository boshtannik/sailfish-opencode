import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: sessionSettingsPage

    property var appWindow

    function currentSession() {
        var id = appWindow.currentSessionId
        var list = appWindow.sessions || []
        for (var i = 0; i < list.length; i++) {
            if (list[i].id === id)
                return list[i]
        }
        return null
    }

    property var session: currentSession()
    property string sessionId: session ? session.id : appWindow.currentSessionId
    property string modelLabel: (session && session.modelID)
                                ? (session.providerID + " / " + session.modelID)
                                : qsTr("no data")
    // Protection against a double tap (the action takes ~a second).
    property bool actionBusy: false
    Timer {
        id: actionGuard
        interval: 1200
        onTriggered: sessionSettingsPage.actionBusy = false
    }

    function runAction(fn) {
        if (actionBusy)
            return
        actionBusy = true
        actionGuard.restart()
        fn()
    }

    function forkSession() {
        if (actionBusy)
            return
        actionBusy = true
        appWindow.forkSession(sessionId)
        actionGuard.restart()
    }

    function todoMark(status) {
        if (status === "completed")
            return "✓"
        if (status === "in_progress")
            return "→"
        if (status === "cancelled")
            return "✗"
        return "☐"
    }

    function todoColor(status) {
        if (status === "completed")
            return Theme.secondaryColor
        if (status === "in_progress")
            return Theme.highlightColor
        if (status === "cancelled")
            return Theme.secondaryColor
        return Theme.primaryColor
    }

    function tokensLabel() {
        if (!session || !session.tokens)
            return "—"
        var t = session.tokens
        return "in " + (t.input || 0) + " / out " + (t.output || 0)
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: pageHeader.height + column.height + Theme.paddingLarge

        PageHeader {
            id: pageHeader
            title: (session && session.title && session.title.length > 0)
                   ? session.title
                   : qsTr("Session")
        }

        Column {
            id: column
            anchors {
                top: pageHeader.bottom
                left: parent.left
                right: parent.right
            }

            SectionHeader { text: qsTr("Session") }

            // This session's mode (agent): build — active work, plan —
            // planning. Per-session; the choice is saved
            // in the dconf registry (see sessionMode/setSessionMode in the root QML).
            ComboBox {
                id: modeCombo
                width: parent.width
                label: qsTr("Working mode")
                description: qsTr("Server agent used by this session")
                menu: ContextMenu {
                    MenuItem { text: qsTr("Build — active work") }
                    MenuItem { text: qsTr("Plan — planning") }
                }
                Binding {
                    target: modeCombo
                    property: "currentIndex"
                    value: appWindow.sessionMode(sessionSettingsPage.sessionId) === "plan" ? 1 : 0
                }
                onCurrentIndexChanged: {
                    var v = modeCombo.currentIndex === 1 ? "plan" : "build"
                    if (v !== appWindow.sessionMode(sessionSettingsPage.sessionId))
                        appWindow.setSessionMode(sessionSettingsPage.sessionId, v)
                }
            }

            BackgroundItem {
                width: parent.width
                contentHeight: Theme.itemSizeSmall
                Label {
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Fork (branch)")
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                onClicked: sessionSettingsPage.forkSession()
            }

            BackgroundItem {
                width: parent.width
                contentHeight: Theme.itemSizeSmall
                Label {
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Rename")
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                onClicked: {
                    var cur = sessionSettingsPage.session
                    appWindow.pageStack.push(appWindow.sessionNameDialogComp, {
                        mode: "rename",
                        sessionId: sessionSettingsPage.sessionId,
                        currentTitle: cur ? (cur.title || "") : ""
                    })
                }
            }

            BackgroundItem {
                width: parent.width
                contentHeight: Theme.itemSizeSmall
                Label {
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Session summary")
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                onClicked: appWindow.pageStack.push(summaryDialog, {
                    title: sessionSettingsPage.session && sessionSettingsPage.session.title
                           ? sessionSettingsPage.session.title : "",
                    sid: sessionSettingsPage.sessionId,
                    model: sessionSettingsPage.modelLabel,
                    cost: sessionSettingsPage.session ? (sessionSettingsPage.session.cost || 0) : 0,
                    tokens: sessionSettingsPage.tokensLabel()
                })
            }

            BackgroundItem {
                id: switchModelItem
                width: parent.width
                contentHeight: Theme.itemSizeMedium
                Column {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    Label {
                        width: parent.width
                        text: qsTr("Switch model")
                        color: switchModelItem.highlighted ? Theme.highlightColor : Theme.primaryColor
                    }
                    // The model the session uses now, with its provider.
                    Label {
                        width: parent.width
                        truncationMode: TruncationMode.Fade
                        text: appWindow.currentModelText
                        color: switchModelItem.highlighted
                               ? Theme.secondaryHighlightColor : Theme.highlightColor
                        font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
                    }
                }
                onClicked: appWindow.openModelPicker()
            }

            // Reasoning level of the current model (only for models that
            // have levels). "Default" leaves the choice to the server.
            ComboBox {
                id: reasoningCombo
                width: parent.width
                visible: appWindow.currentVariants.length > 0
                label: qsTr("Reasoning")
                description: qsTr("How much the model thinks before it answers")
                menu: ContextMenu {
                    Repeater {
                        model: appWindow.variantChoices
                        // Applied on tap only: a refreshed choice list must
                        // never change the level by itself.
                        MenuItem {
                            text: appWindow.variantLabel(modelData)
                            onClicked: if (modelData !== appWindow.currentVariant)
                                           appWindow.setCurrentVariant(modelData)
                        }
                    }
                }
                Binding {
                    target: reasoningCombo
                    property: "currentIndex"
                    value: Math.max(0, appWindow.variantChoices.indexOf(appWindow.currentVariant))
                }
            }

            BackgroundItem {
                width: parent.width
                contentHeight: Theme.itemSizeSmall
                Label {
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Compact (compress history)")
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                onClicked: {
                    var sid = sessionSettingsPage.sessionId
                    remorse.execute(qsTr("Compress history?"), function() {
                        sessionSettingsPage.runAction(function() {
                            appWindow.summarizeSession(sid)
                        })
                    })
                }
            }

            BackgroundItem {
                width: parent.width
                contentHeight: Theme.itemSizeSmall
                visible: appWindow.statusBase(appWindow.statusText) === "busy"
                Label {
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Abort")
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                onClicked: appWindow.abortSession(sessionSettingsPage.sessionId)
            }

            SectionHeader { text: qsTr("TODO tasks") }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: appWindow.todos.length === 0
                text: qsTr("No tasks")
                color: Theme.secondaryColor
                font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
            }

            Repeater {
                model: appWindow.todos

                Item {
                    width: column.width
                    height: Math.max(mark.height, taskLabel.height) + Theme.paddingMedium

                    Label {
                        id: mark
                        x: Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        text: sessionSettingsPage.todoMark(modelData.status)
                        color: sessionSettingsPage.todoColor(modelData.status)
                        font.pixelSize: appWindow.fs(Theme.fontSizeMedium)
                    }

                    Label {
                        id: taskLabel
                        x: mark.x + Theme.itemSizeExtraSmall
                        width: parent.width - x - Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.content
                        wrapMode: Text.Wrap
                        color: sessionSettingsPage.todoColor(modelData.status)
                        font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
                    }
                }
            }

            SectionHeader { text: qsTr("Misc") }

            // Mirrors the official TUI: Settings → Session → Tool calls
            // (show / minimal / hide). Stored globally, applied to all sessions.
            ComboBox {
                id: toolCallsCombo
                label: qsTr("Tool calls")
                description: qsTr("How tool calls are shown in the transcript")
                currentIndex: appWindow.toolCalls === "minimal" ? 1
                              : (appWindow.toolCalls === "hide" ? 2 : 0)
                menu: ContextMenu {
                    MenuItem { text: qsTr("Show (name + argument)") }
                    MenuItem { text: qsTr("Minimal (name only)") }
                    MenuItem { text: qsTr("Hide") }
                }
                // Only write on a real user pick: compare against the current
                // value, so the programmatic `currentIndex` assignment (binding
                // + first init) is a no-op and never pins a stale value.
                onCurrentIndexChanged: {
                    var v = currentIndex === 1 ? "minimal"
                            : (currentIndex === 2 ? "hide" : "show")
                    if (typeof appWindow !== "undefined" && appWindow
                        && appWindow.toolCalls !== v)
                        appWindow.toolCalls = v
                }
            }
        }

        VerticalScrollDecorator {}
    }

    RemorsePopup {
        id: remorse
    }

    Component {
        id: summaryDialog

        Dialog {
            id: dlg
            property string title
            property string sid
            property string model
            property real cost
            property string tokens

            Column {
                width: parent.width

                DialogHeader {
                    acceptText: qsTr("OK")
                    cancelText: ""
                }

                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    wrapMode: Text.Wrap
                    text: dlg.title.length > 0 ? dlg.title : qsTr("(untitled)")
                    color: Theme.primaryColor
                    font.pixelSize: appWindow.fs(Theme.fontSizeMedium)
                }
                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    wrapMode: Text.Wrap
                    text: dlg.sid
                    color: Theme.secondaryColor
                    font.pixelSize: appWindow.fs(Theme.fontSizeExtraSmall)
                }
                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    wrapMode: Text.Wrap
                    text: qsTr("Model: ") + dlg.model
                    color: Theme.secondaryColor
                    font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
                }
                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    wrapMode: Text.Wrap
                    text: qsTr("Cost: $") + dlg.cost.toFixed(4)
                    color: Theme.secondaryColor
                    font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
                }
                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    wrapMode: Text.Wrap
                    text: qsTr("Tokens: ") + dlg.tokens
                    color: Theme.secondaryColor
                    font.pixelSize: appWindow.fs(Theme.fontSizeSmall)
                }
            }
        }
    }
}
