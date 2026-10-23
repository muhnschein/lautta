// SPDX-License-Identifier: LGPL-2.1-or-later
// Actions on the selected items (board DirectorySelect): a docked panel,
// open for as long as selection mode is, of icons spread evenly over its
// width in the long-press menu's order, ending with the button that leaves
// selection (Selection.panelButtons). With one item selected, pushing the
// panel up offers the rest of that menu, Rename and Add to favourites, as
// Silica's media player panel does: in grid view holding an item selects it
// instead of opening the menu (SPEC §15.3). Opaque (UI-9). `owner` is the
// DirectoryView.
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
    readonly property var more: Selection.panelMore(count, writable)

    function run(id) {
        if (!owner)
            return
        if (id === "done")
            owner.endSelection()
        else
            owner.runSelectionAction(id)
    }

    dock: Dock.Bottom
    width: parent ? parent.width : 0
    height: Theme.itemSizeLarge
    // A flickable of its own height, for the push-up menu.
    contentHeight: height
    flickableDirection: Flickable.VerticalFlick
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

    PushUpMenu {
        id: moreMenu

        parent: panel
        visible: panel.more.length > 0

        Repeater {
            model: panel.more

            MenuItem {
                text: modelData.text
                onClicked: panel.run(modelData.id)
            }
        }
    }
}
