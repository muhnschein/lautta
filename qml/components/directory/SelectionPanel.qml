// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../"
import "Selection.js" as Selection

DockedPanel {
    id: panel

    property var owner
    readonly property bool writable: owner ? owner.model.writable : false
    readonly property int count: owner ? owner.model.selectedCount : 0
    readonly property bool waiting: owner ? owner.selectionWaiting : false
    // Ending selection zeroes `count` while panel fades out: no switch to no-selection set on the
    // way.
    property var buttons: Selection.panelButtons(0, false)

    function refreshButtons() {
        if (count > 0 || waiting)
            buttons = Selection.panelButtons(count, writable)
    }

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
    onCountChanged: refreshButtons()
    onWritableChanged: refreshButtons()
    onWaitingChanged: refreshButtons()
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
                key: modelData.id === "done"
                onClicked: panel.run(modelData.id)
            }
        }
    }
}
