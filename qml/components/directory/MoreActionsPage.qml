// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../"
import "../FileIcons.js" as FileIcons
import "ContextActions.js" as Actions
import "Selection.js" as Selection

Page {
    id: page

    property var owner
    property var info: ({})
    property bool selection
    // Fixed when page opens: ending selection must not change rows or header while a dialog takes
    // page's place.
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

            AppIcon {
                id: icon

                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                icon: FileIcons.sized(modelData.icon, iconSize)
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
