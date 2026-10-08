// SPDX-License-Identifier: LGPL-2.1-or-later
// The long-press menu of one item (SPEC §15.3, board DirectoryContext): an
// icon row first, then a short list. Order from the `context_menu` setting,
// filtered by what this item and folder can do. `owner` is the DirectoryView.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../"
import "ContextActions.js" as Actions

ContextMenu {
    id: self

    property var owner
    // The list row, to run its remorse timer in place.
    property Item listItem
    property var info: ({})
    readonly property var spec: Actions.split(Actions.order(App.setting("context_menu")), {
        "isDir": info.isDir,
        "isLocal": App.isLocal(info.uri),
        "writable": owner ? owner.model.writable : false,
        "hasRemote": owner ? owner.hasRemote : false,
        "category": info.category
    })

    IconRow {
        visible: self.spec.row.length > 0
        height: visible ? Theme.itemSizeLarge : 0
        menu: self
        actions: self.spec.row
        onTriggered: if (self.owner) self.owner.runAction(actionId, [self.info.uri], self.info, self.listItem)
    }

    Repeater {
        model: self.spec.list

        MenuItem {
            text: modelData.text
            onClicked: if (self.owner) self.owner.runAction(modelData.id, [self.info.uri], self.info, self.listItem)
        }
    }
}
