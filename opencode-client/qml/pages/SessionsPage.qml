import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: sessionsPage

    property var appWindow

    SilicaListView {
        id: list
        anchors.fill: parent
        model: appWindow.sessions

        header: PageHeader {
            title: qsTr("Sessions")
        }

        delegate: ListItem {
            id: item
            contentHeight: Theme.itemSizeSmall

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                truncationMode: TruncationMode.Fade
                text: (modelData && modelData.title && modelData.title.length > 0)
                      ? modelData.title
                      : (modelData ? modelData.id : "")
                color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
            }

            menu: ContextMenu {
                MenuItem {
                    text: qsTr("Session settings")
                    onClicked: appWindow.openSessionSettings(modelData.id)
                }
                MenuItem {
                    text: qsTr("Rename")
                    onClicked: appWindow.openRenameSessionDialog(modelData.id, modelData.title)
                }
                MenuItem {
                    text: qsTr("Delete")
                    onClicked: {
                        var sid = modelData.id
                        // Row-level remorse via `RemorseItem` (this Silica has no
                        // ListItem.showRemorse), so several sessions can be deleted
                        // in a row (a RemorsePopup would block the screen).
                        remorseItem.execute(item, qsTr("Delete session"), function() {
                            appWindow.deleteSession(sid)
                        })
                    }
                }
            }

            onClicked: appWindow.openSession(modelData.id)

            RemorseItem { id: remorseItem }
        }

        PullDownMenu {
            MenuItem {
                text: qsTr("Delete all sessions")
                enabled: list.count > 0
                onClicked: remorse.execute(qsTr("Delete all sessions?"), function() {
                    appWindow.deleteAllSessions()
                })
            }
            MenuItem {
                text: qsTr("New session")
                onClicked: appWindow.openNewSessionDialog()
            }
        }

        ViewPlaceholder {
            enabled: list.count === 0
            text: qsTr("No sessions yet")
            hintText: qsTr("Pull down to create a new session")
        }

        VerticalScrollDecorator {}
    }

    RemorsePopup {
        id: remorse
    }
}
