import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: statusHeader
    // Compact single-line header plate reflecting daemon connection status.
    height: Theme.itemSizeSmall

    property var appWindow

    // Server-reported retry status: `retry|<next_ms>|<attempt>|<message>`
    // (set by the worker on the `session.status` SSE event, see main.rs).
    // The header itself stays one line ("waiting…"); the details (server
    // message + live countdown + attempt) live in ChatPage's retry banner.
    readonly property var retry: {
        var s = appWindow && appWindow.statusText ? appWindow.statusText : ""
        var prefix = appWindow && appWindow.kServerStatusRetryPrefix
                     ? appWindow.kServerStatusRetryPrefix : "retry|"
        if (s.indexOf(prefix) !== 0)
            return null
        var p = s.substring(prefix.length).split("|")
        return {
            next: parseInt(p[0], 10),
            attempt: parseInt(p[1], 10),
            message: p.slice(2).join("|")
        }
    }

    // Single-line header: [●] <current model ▾> [connecting | idle | busy | waiting… | error]
    // The model name is a button (→ Session settings → Switch model); it
    // fades out when long so the dot and the status always stay visible.
    BackgroundItem {
        id: modelButton
        anchors.fill: parent
        onClicked: appWindow.openModelPickerFromHeader()
    }

    Label {
        id: dot
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        text: "●"
        color: appWindow.statusColor(appWindow.statusText)
        font.pixelSize: appWindow ? appWindow.fs(Theme.fontSizeSmall) : Theme.fontSizeSmall
    }
    Label {
        id: modelLabel
        anchors {
            left: dot.right
            leftMargin: Theme.paddingSmall
            right: statusLabel.left
            rightMargin: Theme.paddingMedium
            verticalCenter: parent.verticalCenter
        }
        text: appWindow ? appWindow.currentModelText + "  ▾" : ""
        truncationMode: TruncationMode.Fade
        color: modelButton.highlighted ? Theme.highlightColor : Theme.primaryColor
        font.pixelSize: appWindow ? appWindow.fs(Theme.fontSizeSmall) : Theme.fontSizeSmall
    }
    Label {
        id: statusLabel
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        text: statusHeader.retry ? qsTr("waiting…") : appWindow.statusText
        width: Math.min(implicitWidth, statusHeader.width * 0.4)
        elide: Text.ElideRight
        color: appWindow.statusColor(appWindow.statusText)
        font.pixelSize: appWindow ? appWindow.fs(Theme.fontSizeExtraSmall) : Theme.fontSizeExtraSmall
    }
}
