// SPDX-License-Identifier: LGPL-2.1-or-later
// The Trash (OPS-8, boards RecentlyDeleted, RecentlyDeletedEmpty): restore,
// delete now, or empty it. Only items deleted from this device's folders
// are kept here; deletes on volumes and servers are permanent (note under the
// header). Images, videos and PDFs show thumbnails.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/operations"
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    readonly property int retentionDays: App.setting("recently_deleted_retention_days") || 30
    readonly property int remorseMs: (App.setting("remorse_seconds") || 5) * 1000

    allowedOrientations: Orientation.All

    TrashModel {
        id: trash
        onFailed: {
            notice.text = ErrorText.message(kind, {})
            notice.show()
        }
        onRestored: {
            //% "Restored"
            notice.text = qsTrId("lautta-trash-restored")
            notice.show()
        }
    }

    Notice {
        id: notice
    }

    RemorsePopup {
        id: emptyRemorse
    }

    Component.onCompleted: trash.reload()

    SilicaListView {
        id: list

        anchors.fill: parent
        model: trash

        PullDownMenu {
            MenuItem {
                enabled: trash.count > 0 && !trash.busy
                //% "Empty trash"
                text: qsTrId("lautta-trash-empty")
                onClicked: {
                    //% "Emptying trash"
                    emptyRemorse.execute(qsTrId("lautta-trash-emptying"), function() { trash.empty() }, page.remorseMs)
                }
            }
        }

        header: Column {
            width: list.width

            PageHeader {
                //% "Trash"
                title: qsTrId("lautta-trash-title")
                //% "Kept for %n days"
                description: qsTrId("lautta-trash-kept", page.retentionDays)
            }

            Item {
                width: parent.width
                height: note.height + Theme.paddingLarge
                visible: trash.count > 0

                Label {
                    id: note

                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    wrapMode: Text.Wrap
                    color: Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeSmall
                    //% "Only files deleted from this device's folders appear here. Deletes on volumes and servers are permanent."
                    text: qsTrId("lautta-trash-note")
                }
            }
        }

        delegate: ListItem {
            id: item

            contentHeight: Theme.itemSizeMedium
            menu: ContextMenu {
                MenuItem {
                    //% "Restore"
                    text: qsTrId("lautta-trash-restore")
                    onClicked: trash.restore(model.trashId)
                }
                MenuItem {
                    //% "Delete now"
                    text: qsTrId("lautta-trash-delete-now")
                    onClicked: {
                        var id = model.trashId
                        //% "Deleting"
                        item.remorseAction(qsTrId("lautta-trash-deleting"), function() { trash.remove(id) }, page.remorseMs)
                    }
                }
                MenuItem {
                    //% "Show original folder"
                    text: qsTrId("lautta-trash-show-folder")
                    onClicked: pageStack.push(Qt.resolvedUrl("DirectoryPage.qml"), { "uri": model.folderUri })
                }
            }

            Item {
                id: glyph

                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.iconSizeMedium
                height: width

                // Image, video or PDF: thumbnail of the stored copy, as folder
                // rows have; other rows keep their glyph (OPS-8, PRV-1, PRV-14).
                FileIcon {
                    anchors.fill: parent
                    visible: model.thumbnailSource.length > 0
                    category: App.categoryOf(model.name)
                    thumbnailSource: model.thumbnailSource
                    mimeType: model.mimeType
                }
                OpIcon {
                    anchors.centerIn: parent
                    visible: model.thumbnailSource.length === 0
                    isDir: model.isDir
                    category: App.categoryOf(model.name)
                }
            }
            Column {
                anchors {
                    left: glyph.right
                    leftMargin: Theme.paddingMedium
                    right: remaining.left
                    rightMargin: Theme.paddingMedium
                    verticalCenter: parent.verticalCenter
                }
                Label {
                    width: parent.width
                    text: model.name
                    color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
                    truncationMode: TruncationMode.Fade
                }
                Label {
                    width: parent.width
                    //% "From %1"
                    text: qsTrId("lautta-trash-from").arg(model.folderName)
                    color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                    truncationMode: TruncationMode.Fade
                }
            }
            Label {
                id: remaining
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                //% "%n days"
                text: qsTrId("lautta-trash-days-left", model.daysLeft)
            }
        }

        ViewPlaceholder {
            enabled: trash.loaded && trash.count === 0
            //% "Trash is empty"
            text: qsTrId("lautta-trash-placeholder")
            //% "Files you delete from this device's folders wait here before they are removed for good."
            hintText: qsTrId("lautta-trash-placeholder-hint")
        }

        VerticalScrollDecorator { }
    }
}
