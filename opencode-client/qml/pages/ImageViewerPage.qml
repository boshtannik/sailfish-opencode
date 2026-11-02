import QtQuick 2.6
import Sailfish.Silica 1.0

// Full-screen view of an attached image: pinch or double-tap to zoom,
// swipe back to close. "Remove" drops the attachment.
Page {
    id: viewer

    property var appWindow
    property string path
    property string name
    // Index in appWindow.attachments, for "Remove".
    property int index: -1

    allowedOrientations: Orientation.All

    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    SilicaFlickable {
        id: flick
        anchors.fill: parent
        contentWidth: Math.max(width, photo.width * photo.scale)
        contentHeight: Math.max(height, photo.height * photo.scale)
        clip: true

        PullDownMenu {
            visible: viewer.index >= 0
            MenuItem {
                text: qsTr("Remove attachment")
                onClicked: {
                    viewer.appWindow.removeAttachment(viewer.index)
                    pageStack.pop()
                }
            }
        }

        PinchArea {
            width: flick.contentWidth
            height: flick.contentHeight
            pinch.target: photo
            pinch.minimumScale: 1
            pinch.maximumScale: 5
            pinch.dragAxis: Pinch.NoDrag

            Image {
                id: photo
                anchors.centerIn: parent
                width: flick.width
                height: flick.height
                fillMode: Image.PreserveAspectFit
                source: viewer.path !== "" ? "file://" + viewer.path : ""
                sourceSize.width: Screen.height * 2
                sourceSize.height: Screen.height * 2
                asynchronous: true
                autoTransform: true
                smooth: true
                Behavior on scale { NumberAnimation { duration: 160; easing.type: Easing.OutQuad } }
            }

            MouseArea {
                anchors.fill: parent
                onDoubleClicked: photo.scale = photo.scale > 1 ? 1 : 2.5
            }
        }

        BusyIndicator {
            anchors.centerIn: parent
            size: BusyIndicatorSize.Large
            running: photo.status === Image.Loading
        }
    }

    Label {
        anchors {
            top: parent.top
            topMargin: Theme.paddingLarge
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        text: viewer.name
        truncationMode: TruncationMode.Fade
        horizontalAlignment: Text.AlignRight
        color: Theme.rgba("white", 0.8)
        font.pixelSize: Theme.fontSizeSmall
    }
}
