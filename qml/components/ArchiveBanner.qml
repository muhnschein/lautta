// SPDX-License-Identifier: LGPL-2.1-or-later
// "Archive contents are read-only. Extract all" (design: ArchiveView). Put it
// at the top of a directory page whose folder lies inside an archive (LOC-5).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

BackgroundItem {
    id: banner

    // Any URI inside the archive location.
    property string uri
    readonly property bool inArchive: uri.length > 0 && Operations.isArchive(uri)

    width: parent ? parent.width : Screen.width
    height: inArchive ? Math.max(Theme.itemSizeSmall, content.height + Theme.paddingLarge) : 0
    visible: inArchive
    onClicked: pageStack.push(Qt.resolvedUrl("../dialogs/ExtractDialog.qml"), {
        "archiveUri": Operations.archiveSource(uri)
    })

    Row {
        id: content
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        spacing: Theme.paddingMedium

        Label {
            width: parent.width - actionLabel.width - Theme.paddingMedium
            wrapMode: Text.Wrap
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeSmall
            //% "Archive contents are read-only."
            text: qsTrId("lautta-archive-readonly")
        }
        Label {
            id: actionLabel
            color: banner.highlighted ? Theme.secondaryHighlightColor : Theme.highlightColor
            font.pixelSize: Theme.fontSizeSmall
            //% "Extract all"
            text: qsTrId("lautta-archive-extract-all")
        }
    }
}
