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

    // Single-line connection header: [●] opencode [connecting | idle | busy | waiting… | error]
    Row {
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        spacing: Theme.paddingSmall

        Label {
            text: "●"
            color: appWindow.statusColor(appWindow.statusText)
            font.pixelSize: Theme.fontSizeSmall
            anchors.verticalCenter: parent.verticalCenter
        }
        Label {
            text: "opencode"
            color: Theme.primaryColor
            font.pixelSize: Theme.fontSizeMedium
            anchors.verticalCenter: parent.verticalCenter
        }
        Label {
            text: statusHeader.retry ? qsTr("waiting…") : appWindow.statusText
            width: Math.min(implicitWidth, statusHeader.width * 0.65)
            elide: Text.ElideRight
            color: appWindow.statusColor(appWindow.statusText)
            font.pixelSize: Theme.fontSizeExtraSmall
            anchors.verticalCenter: parent.verticalCenter
        }
    }
}
