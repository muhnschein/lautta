// SPDX-License-Identifier: LGPL-2.1-or-later
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
        text: Transfers.activeCount > 0
              //% "Activity (%1)"
              ? qsTrId("lautta-dir-menu-activity-count").arg(Transfers.activeCount)
              //% "Activity"
              : qsTrId("lautta-dir-menu-activity")
        onClicked: menu.owner.activity()
    }
    MenuItem {
        visible: !menu.selecting
        //% "View options"
        text: qsTrId("lautta-dir-menu-view-options")
        onClicked: menu.owner.viewOptions()
    }
    MenuItem {
        visible: !menu.selecting
        //% "Search"
        text: qsTrId("lautta-dir-menu-search")
        onClicked: menu.owner.searchHere()
    }
    MenuItem {
        visible: !menu.selecting
        //% "Select"
        text: qsTrId("lautta-dir-menu-select")
        onClicked: menu.owner.startSelection()
    }
    MenuItem {
        visible: !menu.selecting && menu.writable && App.clipboardCount > 0 && App.canPasteInto(owner ? owner.uri : "")
        //% "Paste %n items"
        text: qsTrId("lautta-dir-menu-paste", App.clipboardCount)
        onClicked: menu.owner.paste()
    }
    MenuItem {
        visible: !menu.selecting && menu.writable
        //% "New"
        text: qsTrId("lautta-dir-menu-new")
        onClicked: menu.owner.newFolder()
    }
}
