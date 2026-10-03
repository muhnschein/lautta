// SPDX-License-Identifier: LGPL-2.1-or-later
// Actions on the selected items (board DirectorySelect): a docked panel of
// icons, with "more" opening a second row. `owner` is the DirectoryView.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../"
import "ContextActions.js" as Actions

DockedPanel {
    id: panel

    property var owner
    property bool moreOpen
    readonly property bool writable: owner ? owner.model.writable : false
    readonly property int count: owner ? owner.model.selectedCount : 0
    readonly property bool hasPermissions: owner ? owner.model.hasCapability("Permissions") : false
    readonly property bool otherPane: owner ? owner.otherUri.length > 0 : false

    function _run(id) {
        moreOpen = false
        if (owner)
            owner.runSelectionAction(id)
    }

    dock: Dock.Bottom
    width: parent ? parent.width : 0
    height: content.height
    open: owner ? owner.selecting && count > 0 : false
    onOpenChanged: if (!open) moreOpen = false

    Column {
        id: content

        width: panel.width

        IconRow {
            visible: panel.moreOpen
            height: visible ? Theme.itemSizeLarge : 0
            menu: null
            actions: Actions.moreActions(panel.count === 1, panel.hasPermissions, panel.otherPane)
            onTriggered: panel._run(actionId)
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter

            DockButton {
                icon: "dir-copy"
                //% "Copy"
                description: qsTrId("lautta-dir-sel-copy")
                onClicked: panel._run("copy")
            }
            DockButton {
                visible: panel.writable
                icon: "dir-cut"
                //% "Cut"
                description: qsTrId("lautta-dir-sel-cut")
                onClicked: panel._run("cut")
            }
            DockButton {
                visible: panel.writable
                icon: "image://theme/icon-m-delete"
                //% "Delete"
                description: qsTrId("lautta-dir-sel-delete")
                onClicked: panel._run("delete")
            }
            DockButton {
                icon: "image://theme/icon-m-share"
                //% "Share"
                description: qsTrId("lautta-dir-sel-share")
                onClicked: panel._run("share")
            }
            DockButton {
                icon: "dir-more"
                accent: panel.moreOpen
                //% "More"
                description: qsTrId("lautta-dir-sel-more")
                onClicked: panel.moreOpen = !panel.moreOpen
            }
        }
    }
}
