// SPDX-License-Identifier: LGPL-2.1-or-later
// The long-press menu of one item (SPEC §15.3, board DirectoryContext): plain
// Silica rows, filtered by what this item and folder can do. More actions,
// the last row, opens a page of the rarer actions for this item
// (MoreActionsPage.qml), as Sailfish leads on from a menu. `owner` is the
// DirectoryView.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "ContextActions.js" as Actions

ContextMenu {
    id: self

    property var owner
    // The list row, to run its remorse timer in place.
    property Item listItem
    property var info: ({})

    Repeater {
        model: Actions.menu({
            "isDir": self.info.isDir,
            "writable": self.owner ? self.owner.model.writable : false
        })

        MenuItem {
            text: modelData.text
            onClicked: {
                if (self.owner)
                    self.owner.runAction(modelData.id, [self.info.uri], self.info, self.listItem)
            }
        }
    }
}
