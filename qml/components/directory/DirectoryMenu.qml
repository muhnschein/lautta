// SPDX-License-Identifier: LGPL-2.1-or-later
// The pulley menu of a folder (boards DirectoryPulley, DirectorySelectPulley).
// `owner` is the DirectoryView. Entries that do not apply are not shown
// (UI-8), never disabled.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

PullDownMenu {
    id: menu

    property var owner
    readonly property bool selecting: owner ? owner.selecting : false
    readonly property bool writable: owner ? owner.model.writable : false

    MenuItem {
        visible: menu.selecting
        //% "Select all"
        text: qsTrId("lautta-dir-menu-select-all")
        onClicked: menu.owner.model.selectAll()
    }
    MenuItem {
        visible: menu.selecting
        //% "Clear selection"
        text: qsTrId("lautta-dir-menu-clear-selection")
        onClicked: menu.owner.endSelection()
    }
    MenuItem {
        visible: !menu.selecting
        //% "Select"
        text: qsTrId("lautta-dir-menu-select")
        onClicked: menu.owner.selectionMode = true
    }
    MenuItem {
        visible: !menu.selecting && menu.writable
        //% "New folder or file"
        text: qsTrId("lautta-dir-menu-new")
        onClicked: menu.owner.newItem()
    }
    MenuItem {
        visible: !menu.selecting && menu.writable && App.clipboardCount > 0 && App.canPasteInto(owner ? owner.uri : "")
        //% "Paste %n items"
        text: qsTrId("lautta-dir-menu-paste", App.clipboardCount)
        onClicked: menu.owner.paste()
    }
    MenuItem {
        visible: !menu.selecting
        //% "Search here"
        text: qsTrId("lautta-dir-menu-search")
        onClicked: menu.owner.searchHere()
    }
    MenuItem {
        visible: !menu.selecting
        //% "View options"
        text: qsTrId("lautta-dir-menu-view-options")
        onClicked: menu.owner.viewOptions()
    }
    MenuItem {
        visible: !menu.selecting
        //% "Refresh"
        text: qsTrId("lautta-dir-menu-refresh")
        onClicked: menu.owner.model.refresh()
    }
}
