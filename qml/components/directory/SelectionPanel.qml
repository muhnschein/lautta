// SPDX-License-Identifier: LGPL-2.1-or-later
// Actions on the selected items (board DirectorySelect): a docked panel of
// icons, open for as long as selection mode is, ending with the button that
// leaves it. `owner` is the DirectoryView.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../"

DockedPanel {
    id: panel

    property var owner
    readonly property bool writable: owner ? owner.model.writable : false
    readonly property int count: owner ? owner.model.selectedCount : 0
    readonly property bool any: count > 0

    function _run(id) {
        if (owner)
            owner.runSelectionAction(id)
    }

    dock: Dock.Bottom
    width: parent ? parent.width : 0
    height: content.height
    open: owner ? owner.selecting : false

    Row {
        id: content

        anchors.horizontalCenter: parent.horizontalCenter

        DockButton {
            visible: panel.any
            icon: "dir-copy"
            //% "Copy"
            description: qsTrId("lautta-dir-sel-copy")
            onClicked: panel._run("copy")
        }
        DockButton {
            visible: panel.any && panel.writable
            icon: "dir-cut"
            //% "Cut"
            description: qsTrId("lautta-dir-sel-cut")
            onClicked: panel._run("cut")
        }
        DockButton {
            visible: panel.any && panel.writable
            icon: "image://theme/icon-m-delete"
            //% "Delete"
            description: qsTrId("lautta-dir-sel-delete")
            onClicked: panel._run("delete")
        }
        DockButton {
            visible: panel.any
            icon: "image://theme/icon-m-share"
            //% "Share"
            description: qsTrId("lautta-dir-sel-share")
            onClicked: panel._run("share")
        }
        DockButton {
            visible: panel.any
            icon: "image://theme/icon-m-file-compressed"
            //% "Compress"
            description: qsTrId("lautta-dir-sel-compress")
            onClicked: panel._run("compress")
        }
        DockButton {
            icon: "image://theme/icon-m-dismiss"
            //% "Stop selecting"
            description: qsTrId("lautta-dir-sel-done")
            onClicked: if (panel.owner) panel.owner.endSelection()
        }
    }
}
