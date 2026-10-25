import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: statusHeader
    // Compact single-line header plate reflecting daemon connection status.
    height: Theme.itemSizeSmall

    property var appWindow

    // Server-reported retry status: `retry|<next_ms>|<attempt>|<message>`
    // (set by the worker on the `session.status` SSE event, see main.rs).
    // Mirrors the opencode TUI line: "message — retrying in N s - attempt #A".
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
    // Ticks once per second so the countdown below is live.
    property int tick: 0
    Timer {
        interval: 1000
        repeat: true
        running: statusHeader.retry !== null
        onTriggered: statusHeader.tick++
    }
    readonly property int retrySeconds: {
        if (!statusHeader.retry) return 0
        statusHeader.tick // dependency: re-evaluate every second
        return Math.max(0, Math.ceil((statusHeader.retry.next - Date.now()) / 1000))
    }
    readonly property string remainingText: {
        if (!statusHeader.retry) return ""
        if (appWindow && appWindow.formatRemainingTime)
            return appWindow.formatRemainingTime(statusHeader.retrySeconds)
        return statusHeader.retrySeconds + "s"
    }
    readonly property string retryText: {
        if (!statusHeader.retry) return ""
        var line = qsTr("retrying")
        if (statusHeader.retrySeconds > 0)
            line += " " + qsTr("in %1").arg(statusHeader.remainingText)
        var tail = qsTr("attempt #%1").arg(statusHeader.retry.attempt)
        var msg = statusHeader.retry.message
        if (msg !== "")
            return msg + " — " + line + " - " + tail
        return line + " - " + tail
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
