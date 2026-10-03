// SPDX-License-Identifier: LGPL-2.1-or-later
// Extract a whole archive (PRV-10, XFR-3): into a new folder named after the
// archive or straight into the chosen folder.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/operations"
import "../components/operations/Json.js" as Json

Dialog {
    id: dialog

    // The archive file (not a location inside it).
    property string archiveUri
    property string destUri: archiveUri.length > 0 ? App.parentUri(archiveUri) : ""
    property var archive: ({})
    property bool loaded

    function folderName() {
        return archive.folderName || Json.stem(App.nameOf(archiveUri))
    }

    function contentsText() {
        //% "%n files"
        var files = qsTrId("lautta-extract-files", archive.files || 0)
        //% "%n folders"
        var dirs = qsTrId("lautta-extract-folders", archive.dirs || 0)
        //% "%1, %2"
        return qsTrId("lautta-extract-contents").arg(files).arg(dirs)
    }

    function targetUri() {
        return intoBox.currentIndex === 0 ? App.childUri(destUri, folderName()) : destUri
    }

    canAccept: loaded && destUri.length > 0 && targetUri().length > 0

    onAccepted: {
        if (openSwitch.checked)
            Operations.extractAndOpen(archiveUri, targetUri())
        else
            Operations.extract(archiveUri, targetUri())
    }

    Component.onCompleted: Operations.inspectArchive(archiveUri)

    Connections {
        target: Operations
        onArchiveInspected: {
            if (uri !== dialog.archiveUri)
                return
            dialog.archive = JSON.parse(infoJson)
            dialog.loaded = true
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Extract"
                acceptText: qsTrId("lautta-extract-accept")
            }
            DialogSubtitle {
                //% "Extract %1"
                text: qsTrId("lautta-extract-title").arg(App.nameOf(dialog.archiveUri))
            }

            BusyIndicator {
                anchors.horizontalCenter: parent.horizontalCenter
                size: BusyIndicatorSize.Small
                running: !dialog.loaded
                visible: running
            }
            DetailItem {
                visible: dialog.loaded
                //% "Contents"
                label: qsTrId("lautta-extract-contents-label")
                value: dialog.contentsText()
            }
            DetailItem {
                visible: dialog.loaded
                //% "Size"
                label: qsTrId("lautta-extract-size")
                //% "%1 uncompressed"
                value: qsTrId("lautta-extract-uncompressed").arg(Format.formatFileSize(dialog.archive.bytes || 0))
            }
            ComboBox {
                id: intoBox
                //% "Into"
                label: qsTrId("lautta-extract-into")
                menu: ContextMenu {
                    MenuItem {
                        //% "New folder '%1'"
                        text: qsTrId("lautta-extract-new-folder").arg(dialog.folderName())
                    }
                    MenuItem {
                        //% "The destination itself"
                        text: qsTrId("lautta-extract-here")
                    }
                }
            }
            BackgroundItem {
                width: parent.width
                height: Theme.itemSizeMedium
                onClicked: {
                    var picker = pageStack.push(Qt.resolvedUrl("FolderPickerDialog.qml"), {
                        //% "Extract to"
                        "title": qsTrId("lautta-extract-pick"),
                        //% "Select"
                        "acceptText": qsTrId("lautta-extract-pick-accept"),
                        "startUri": dialog.destUri
                    })
                    picker.accepted.connect(function() { dialog.destUri = picker.selectedUri })
                }

                OpIcon {
                    id: folderIcon
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    isDir: true
                }
                Column {
                    anchors {
                        left: folderIcon.right
                        leftMargin: Theme.paddingMedium
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    Label {
                        width: parent.width
                        text: Operations.displayPath(dialog.destUri)
                        truncationMode: TruncationMode.Fade
                    }
                    Label {
                        width: parent.width
                        color: Theme.secondaryColor
                        font.pixelSize: Theme.fontSizeSmall
                        //% "Destination"
                        text: qsTrId("lautta-extract-destination")
                    }
                }
            }
            TextSwitch {
                id: openSwitch
                checked: true
                //% "Open folder when done"
                text: qsTrId("lautta-extract-open")
            }
        }
        VerticalScrollDecorator { }
    }
}
