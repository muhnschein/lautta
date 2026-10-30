// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "ContextActions.js" as Actions

ContextMenu {
    id: self

    property var owner
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
