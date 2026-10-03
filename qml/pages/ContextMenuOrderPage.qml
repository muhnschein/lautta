// SPDX-License-Identifier: LGPL-2.1-or-later
// Context menu order (SPEC §18; design: ContextMenuOrder): the first five
// entries form the icon row, the others the list below it. Stored in the
// `context_menu` setting.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/search"
import "../components/search/ContextMenuOrder.js" as Order

Page {
    id: page

    readonly property var order: {
        var dependency = App.settingsJson
        return Order.parse(App.setting("context_menu"))
    }

    function save(list) {
        App.setSetting("context_menu", JSON.stringify(list))
    }

    allowedOrientations: Orientation.All

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            MenuItem {
                //% "Restore default order"
                text: qsTrId("lautta-menuorder-reset")
                onClicked: page.save(Order.DEFAULT_ORDER)
            }
        }

        Column {
            id: column

            width: parent.width

            PageHeader {
                //% "Context menu"
                title: qsTrId("lautta-menuorder-title")
                //% "Press and hold an entry to move it"
                description: qsTrId("lautta-menuorder-description")
            }

            SectionHeader {
                //% "Icon row · up to 5"
                text: qsTrId("lautta-menuorder-row")
            }
            Repeater {
                model: page.order.slice(0, Order.ICON_ROW)

                ContextMenuEntry {
                    entryId: modelData
                    entryIndex: index
                    entryCount: page.order.length
                    onMoveUp: page.save(Order.moveUp(page.order, entryIndex))
                    onMoveDown: page.save(Order.moveDown(page.order, entryIndex))
                    onMoveToList: page.save(Order.toList(page.order, entryIndex))
                }
            }

            SectionHeader {
                //% "List"
                text: qsTrId("lautta-menuorder-list")
            }
            Repeater {
                model: page.order.slice(Order.ICON_ROW)

                ContextMenuEntry {
                    entryId: modelData
                    entryIndex: index + Order.ICON_ROW
                    entryCount: page.order.length
                    onMoveUp: page.save(Order.moveUp(page.order, entryIndex))
                    onMoveDown: page.save(Order.moveDown(page.order, entryIndex))
                    onMoveToRow: page.save(Order.toRow(page.order, entryIndex))
                }
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
                //% "Entries a location doesn't support are left out automatically."
                text: qsTrId("lautta-menuorder-note")
            }
        }

        VerticalScrollDecorator { }
    }
}
