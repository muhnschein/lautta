// SPDX-License-Identifier: LGPL-2.1-or-later
// More actions of one item (SPEC OPS-12, board DirectoryContext) or of the
// selected items (§15.3, board DirectorySelect): the rarer actions on a page
// of their own, as Sailfish leads on from a menu or a bar to an action page.
// The header names the item, or counts the selected ones; a row is the
// action's theme icon and name. Tapping a row runs the action and leaves the
// page: a dialog the action opens takes the page's place
// (DirectoryView.openDialog), so leaving it returns to the folder. `owner` is
// the DirectoryView, `info` describes the item as its row does. For the
// selection (`selection`), `info` is its first item and `count` how many
// there are: the rows are Selection.moreRows, run on the whole selection.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "ContextActions.js" as Actions
import "Selection.js" as Selection

Page {
    id: page

    property var owner
    property var info: ({})
    property bool selection
    // Fixed when the page opens: ending selection must not change the rows
    // or the header while a dialog takes the page's place.
    property int count: 1
    readonly property var ctx: ({
        "isDir": info.isDir,
        "writable": owner ? owner.model.writable : false
    })
    readonly property var actions: selection ? Selection.moreRows(count, ctx) : Actions.more(ctx)

    function choose(id) {
        if (owner && selection)
            owner.runSelectionAction(id)
        else if (owner)
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
            title: page.count === 1 ? App.nameOf(page.info.uri) : qsTrId("lautta-dir-selected", page.count)
        }

        delegate: BackgroundItem {
            id: row

            objectName: "moreRow"
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
