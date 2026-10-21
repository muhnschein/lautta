// SPDX-License-Identifier: LGPL-2.1-or-later
// The long-press menu of one item (SPEC §15.3, board DirectoryContext): plain
// Silica rows, filtered by what this item and folder can do. More actions,
// the last row, keeps the menu open and puts the rarer actions for this item
// in its rows' place. `owner` is the DirectoryView.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "ContextActions.js" as Actions

ContextMenu {
    id: self

    property var owner
    // The list row, to run its remorse timer in place.
    property Item listItem
    property var info: ({})
    // True once More actions was tapped. The menu is made again for every
    // long-press (ListItem destroys it when it closes), so this starts false.
    property bool expanded
    readonly property var itemContext: ({
        "isDir": info.isDir,
        "writable": owner ? owner.model.writable : false
    })
    readonly property var actions: expanded ? Actions.more(itemContext) : Actions.menu(itemContext)

    function activate(id) {
        // Silica calls the row's clicked() before it reads closeOnActivation
        // (ContextMenu._activatedMenuItem), so the row decides here whether
        // this tap closes the menu.
        closeOnActivation = id !== "more"
        if (id === "more")
            expanded = true
        else if (owner)
            owner.runAction(id, [info.uri], info, listItem)
    }

    Repeater {
        model: self.actions

        MenuItem {
            text: modelData.text
            onClicked: self.activate(modelData.id)
        }
    }
}
