// SPDX-License-Identifier: LGPL-2.1-or-later
// Actions on the selected items (board DirectorySelect): a docked panel,
// open for as long as selection mode is, of icons spread evenly over its
// width in the long-press menu's order, then More actions, which opens the
// rest on a page of its own (MoreActionsPage.qml), and the button that leaves
// selection (Selection.panelButtons). The same buttons for one item or
// several (SPEC §15.3). Opaque (UI-10). `owner` is the DirectoryView.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../"
import "Selection.js" as Selection

DockedPanel {
    id: panel

    property var owner
    readonly property bool writable: owner ? owner.model.writable : false
    readonly property int count: owner ? owner.model.selectedCount : 0
    readonly property var buttons: Selection.panelButtons(count, writable)

    function run(id) {
        if (!owner)
            return
        if (id === "done")
            owner.endSelection()
        else if (id === "more")
            owner.moreSelection()
        else
            owner.runSelectionAction(id)
    }

    dock: Dock.Bottom
    width: parent ? parent.width : 0
    height: Theme.itemSizeLarge
    open: owner ? owner.selecting : false
    background: BarBackground { }

    Row {
        id: content

        width: panel.width
        height: panel.height

        Repeater {
            model: panel.buttons

            DockButton {
                width: content.width / panel.buttons.length
                height: content.height
                icon: modelData.icon
                description: modelData.text
                onClicked: panel.run(modelData.id)
            }
        }
    }
}
