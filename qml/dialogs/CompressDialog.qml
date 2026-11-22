// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/operations/Json.js" as Json

// Compressed file lands beside the items.
Dialog {
    id: dialog

    property var uris: []
    property string format: "zip"
    readonly property string folderUri: uris.length > 0 ? App.parentUri(uris[0]) : ""

    readonly property var formats: [
        //% "Opens on every phone and computer"
        { "id": "zip", "name": "Zip", "line": qsTrId("lautta-compress-zip-line") },
        //% "Smaller. Some phones and computers need an app"
        { "id": "7z", "name": "7z", "line": qsTrId("lautta-compress-7z-line") },
        //% "Smallest. Made for Linux and Mac"
        { "id": "tar.xz", "name": "tar.xz", "line": qsTrId("lautta-compress-tarxz-line") },
        //% "Fast. Made for Linux and Mac"
        { "id": "tar.gz", "name": "tar.gz", "line": qsTrId("lautta-compress-targz-line") }
    ]

    function defaultName() {
        if (uris.length === 0)
            return ""
        if (uris.length === 1)
            return Json.stem(App.nameOf(uris[0]))
        var folder = App.nameOf(folderUri)
        //% "Compressed"
        return folder.length > 0 ? folder : qsTrId("lautta-compress-default-name")
    }

    canAccept: nameField.text.trim().length > 0 && folderUri.length > 0

    onAccepted: Operations.compress(JSON.stringify(uris), folderUri, nameField.text.trim(), format)
    // Set once: bound, Silica reports a canAccept binding loop.
    Component.onCompleted: nameField.text = defaultName()

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

            TextField {
                id: nameField
                width: parent.width
                //% "Name"
                label: qsTrId("lautta-compress-name")
                placeholderText: label
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }

            SectionHeader {
                //% "Format"
                text: qsTrId("lautta-compress-format")
            }

            Repeater {
                model: dialog.formats

                BackgroundItem {
                    id: row

                    readonly property bool picked: dialog.format === modelData.id

                    objectName: "compressFormat"
                    width: column.width
                    height: Math.max(Theme.itemSizeMedium, texts.height + 2 * Theme.paddingMedium)
                    onClicked: dialog.format = modelData.id

                    Column {
                        id: texts
                        x: Theme.horizontalPageMargin
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter

                        Label {
                            width: parent.width
                            text: modelData.name
                            color: row.picked || row.highlighted ? Theme.highlightColor : Theme.primaryColor
                        }
                        Label {
                            width: parent.width
                            text: modelData.line
                            wrapMode: Text.Wrap
                            font.pixelSize: Theme.fontSizeExtraSmall
                            color: row.picked || row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                        }
                    }
                }
            }
        }
        VerticalScrollDecorator { }
    }
}
