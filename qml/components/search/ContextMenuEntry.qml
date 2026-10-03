// SPDX-License-Identifier: LGPL-2.1-or-later
// One entry of the context menu order page: its name, its icon on the right
// and a menu (press and hold) that moves it.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../"
import "ContextMenuLabels.js" as Labels
import "ContextMenuOrder.js" as Order

ListItem {
    id: item

    property string entryId
    // Position in the whole order; the first Order.ICON_ROW are the icon row.
    property int entryIndex
    property int entryCount

    signal moveUp()
    signal moveDown()
    signal moveToRow()
    signal moveToList()

    readonly property bool inRow: entryIndex < Order.ICON_ROW

    contentHeight: Theme.itemSizeSmall
    menu: Component {
        ContextMenu {
            MenuItem {
                visible: item.entryIndex > 0
                //% "Move up"
                text: qsTrId("lautta-menuorder-up")
                onClicked: item.moveUp()
            }
            MenuItem {
                visible: item.entryIndex < item.entryCount - 1
                //% "Move down"
                text: qsTrId("lautta-menuorder-down")
                onClicked: item.moveDown()
            }
            MenuItem {
                visible: !item.inRow
                //% "Move to icon row"
                text: qsTrId("lautta-menuorder-to-row")
                onClicked: item.moveToRow()
            }
            MenuItem {
                visible: item.inRow && item.entryCount > Order.ICON_ROW
                //% "Move to list"
                text: qsTrId("lautta-menuorder-to-list")
                onClicked: item.moveToList()
            }
        }
    }

    Label {
        anchors {
            left: parent.left
            right: icon.left
            leftMargin: Theme.horizontalPageMargin
            rightMargin: Theme.paddingLarge
            verticalCenter: parent.verticalCenter
        }
        text: Labels.label(item.entryId)
        truncationMode: TruncationMode.Fade
        color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
    }

    AppIcon {
        id: icon

        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        icon: Labels.icon(item.entryId)
        small: true
        highlighted: item.highlighted
    }
}
