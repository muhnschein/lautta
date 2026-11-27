// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "ContextActions.js" as ContextActions

PullDownMenu {
    id: menu

    property var owner
    readonly property bool selecting: owner ? owner.selecting : false
    readonly property bool picking: owner ? owner.picking : false
    readonly property bool browsing: !selecting && !picking
    readonly property bool writable: owner ? owner.model.writable : false
    readonly property bool readable: owner ? owner.model.readable : false
    readonly property bool makes: picking && writable && !owner.atRoot
    readonly property string archiveUri: owner ? Operations.archiveSource(owner.uri) : ""
    property bool favourite

    visible: !picking || makes
    onActiveChanged: if (active && owner) favourite = App.isFavourite(owner.uri)

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
        visible: menu.browsing
        text: Transfers.activeCount > 0
              //% "Activity (%1)"
              ? qsTrId("lautta-dir-menu-activity-count").arg(Transfers.activeCount)
              //% "Activity"
              : qsTrId("lautta-dir-menu-activity")
        onClicked: menu.owner.activity()
    }
    MenuItem {
        visible: menu.browsing
        //% "View options"
        text: qsTrId("lautta-dir-menu-view-options")
        onClicked: menu.owner.viewOptions()
    }
    MenuItem {
        visible: menu.browsing && menu.readable
        //% "Search"
        text: qsTrId("lautta-dir-menu-search")
        onClicked: menu.owner.searchHere()
    }
    MenuItem {
        visible: menu.browsing && menu.readable
        //% "Select"
        text: qsTrId("lautta-dir-menu-select")
        onClicked: menu.owner.startSelection()
    }
    MenuItem {
        objectName: "folderMenuFavourite"
        visible: menu.browsing
        text: ContextActions.text("favourite", { "favourite": menu.favourite })
        onClicked: menu.owner.runAction("favourite", [menu.owner.uri], {}, null)
    }
    MenuItem {
        visible: menu.browsing && (owner ? owner.canPaste : false)
        //% "Paste %n items"
        text: qsTrId("lautta-dir-menu-paste", App.clipboardCount)
        onClicked: menu.owner.paste()
    }
    MenuItem {
        visible: menu.browsing && menu.writable
        //% "Add"
        text: qsTrId("lautta-dir-menu-add")
        onClicked: menu.owner.add()
    }
    MenuItem {
        objectName: "folderMenuNewFolder"
        visible: menu.makes
        //% "New folder"
        text: qsTrId("lautta-dir-menu-new-folder")
        onClicked: menu.owner.newFolder()
    }
    MenuItem {
        objectName: "folderMenuExtract"
        visible: menu.browsing && menu.archiveUri.length > 0 && App.canAddTo(App.parentUri(menu.archiveUri))
        text: qsTrId("lautta-dir-act-extract")
        onClicked: Operations.extract(menu.archiveUri, "")
    }
}
