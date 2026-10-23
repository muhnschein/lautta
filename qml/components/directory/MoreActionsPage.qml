// SPDX-License-Identifier: LGPL-2.1-or-later
// More actions of one item (SPEC OPS-12, board DirectoryContext): the rarer
// actions on a page of their own, as Sailfish leads on from a menu to an
// action page rather than changing the menu's rows. The header names the
// item; a row is the action's theme icon and name. Tapping a row runs the
// action and leaves the page: a dialog the action opens takes the page's
// place (DirectoryView.openDialog), so leaving it returns to the folder.
// `owner` is the DirectoryView, `info` describes the item as its row does.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "ContextActions.js" as Actions

Page {
    id: page

    property var owner
    property var info: ({})
    readonly property var actions: Actions.more({
        "isDir": info.isDir,
        "writable": owner ? owner.model.writable : false
    })

    function choose(id) {
        if (owner)
            owner.runAction(id, [info.uri], info, null)
        // Nothing took the page's place: leave it here.
        if (pageStack.currentPage === page)
            pageStack.pop()
    }

    allowedOrientations: Orientation.All

    SilicaListView {
        anchors.fill: parent
        model: page.actions

        header: PageHeader {
            title: App.nameOf(page.info.uri)
        }

        delegate: BackgroundItem {
            id: row

            height: Theme.itemSizeSmall
            onClicked: page.choose(modelData.id)

            Icon {
                id: icon

                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                source: modelData.icon
                highlighted: row.highlighted
            }

            Label {
                anchors {
                    left: icon.right
                    leftMargin: Theme.paddingLarge
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                text: modelData.text
                truncationMode: TruncationMode.Fade
                color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
            }
        }

        VerticalScrollDecorator { }
    }
}
