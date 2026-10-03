// SPDX-License-Identifier: LGPL-2.1-or-later
// Compress a selection to zip or tar.gz (PRV-11). The archive is made in the
// background; Operations reports progress and the result.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/operations"
import "../components/operations/Json.js" as Json

Dialog {
    id: dialog

    // The URIs of the items to pack.
    property var uris: []
    property string destUri: uris.length > 0 ? App.parentUri(uris[0]) : ""
    property real sourceBytes: -1
    property bool measuring

    readonly property var formats: ["zip", "tar.gz"]

    function defaultName() {
        if (uris.length === 0)
            return ""
        if (uris.length === 1)
            return Json.stem(App.nameOf(uris[0]))
        var folder = App.nameOf(App.parentUri(uris[0]))
        //% "Archive"
        return folder.length > 0 ? folder : qsTrId("lautta-compress-default-name")
    }

    canAccept: nameField.text.trim().length > 0 && destUri.length > 0

    onAccepted: Operations.compress(JSON.stringify(uris), destUri, nameField.text.trim(),
                                    formats[formatBox.currentIndex])

    Component.onCompleted: {
        if (uris.length > 0) {
            measuring = true
            Operations.measure(JSON.stringify(uris))
        }
    }

    Connections {
        target: Operations
        onMeasured: {
            if (!dialog.measuring)
                return
            dialog.measuring = false
            dialog.sourceBytes = JSON.parse(infoJson).bytes
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                //% "Compress"
                acceptText: qsTrId("lautta-compress-accept")
            }
            DialogSubtitle {
                //% "Compress %n items"
                text: qsTrId("lautta-compress-count", dialog.uris.length)
            }

            TextField {
                id: nameField
                width: parent.width
                //% "Name"
                label: qsTrId("lautta-compress-name")
                placeholderText: label
                text: dialog.defaultName()
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }
            ComboBox {
                id: formatBox
                //% "Format"
                label: qsTrId("lautta-compress-format")
                menu: ContextMenu {
                    MenuItem { text: "zip" }
                    MenuItem { text: "tar.gz" }
                }
            }

            SectionHeader {
                //% "Save to"
                text: qsTrId("lautta-compress-save-to")
            }
            BackgroundItem {
                width: parent.width
                height: Theme.itemSizeMedium
                onClicked: {
                    var picker = pageStack.push(Qt.resolvedUrl("FolderPickerDialog.qml"), {
                        //% "Save archive to"
                        "title": qsTrId("lautta-compress-pick"),
                        //% "Select"
                        "acceptText": qsTrId("lautta-compress-pick-accept"),
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
                Label {
                    anchors {
                        left: folderIcon.right
                        leftMargin: Theme.paddingMedium
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    text: Operations.displayPath(dialog.destUri)
                    truncationMode: TruncationMode.Fade
                }
            }
            DetailItem {
                visible: dialog.sourceBytes >= 0
                //% "Size before compression"
                label: qsTrId("lautta-compress-size")
                value: Format.formatFileSize(dialog.sourceBytes)
            }
        }
        VerticalScrollDecorator { }
    }
}
