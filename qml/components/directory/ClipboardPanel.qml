// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../"

DockedPanel {
    id: panel

    property var owner
    readonly property string fromText: {
        var count = App.clipboardCount
        if (count === 0)
            return ""
        var items = JSON.parse(App.clipboardJson())
        if (items.length === 0)
            return ""
        var folder = App.parentUri(items[0])
        var place = App.locationName(folder)
        var name = App.nameOf(folder)
        //% "from %1"
        return qsTrId("lautta-dir-clip-from").arg(name === place || name.length === 0 ? place : place + " › " + name)
    }

    dock: Dock.Bottom
    width: parent ? parent.width : 0
    height: Theme.itemSizeLarge
    open: owner ? App.clipboardCount > 0 && !owner.selecting : false
    background: BarBackground { }

    Row {
        anchors {
            fill: parent
            leftMargin: Theme.horizontalPageMargin
        }

        Column {
            width: parent.width - pasteButton.width - clearButton.width
            anchors.verticalCenter: parent.verticalCenter

            Label {
                width: parent.width
                text: App.clipboardCut
                      //% "%n items cut"
                      ? qsTrId("lautta-dir-clip-cut", App.clipboardCount)
                      //% "%n items copied"
                      : qsTrId("lautta-dir-clip-copied", App.clipboardCount)
                font.pixelSize: Theme.fontSizeSmall
                truncationMode: TruncationMode.Fade
            }
            Label {
                width: parent.width
                text: panel.fromText
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                truncationMode: TruncationMode.Fade
            }
        }

        DockButton {
            id: pasteButton

            visible: owner ? owner.model.writable && App.canPasteInto(owner.uri) : false
            width: visible ? Theme.itemSizeMedium : 0
            icon: "image://theme/icon-m-clipboard"
            accent: true
            //% "Paste here"
            description: qsTrId("lautta-dir-clip-paste")
            onClicked: if (owner) owner.paste()
        }
        DockButton {
            id: clearButton

            icon: "image://theme/icon-m-dismiss"
            //% "Clear clipboard"
            description: qsTrId("lautta-dir-clip-clear")
            onClicked: App.clearClipboard()
        }
    }
}
