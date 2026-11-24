// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../"
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
        "changeable": owner ? owner.model.changeable : false,
        "readable": owner ? owner.model.readable : false,
        "writable": owner ? owner.model.writable : false,
        "archive": count === 1 && info.isDir === false && App.viewerFor(info.uri || "", info.mimeType || "") === "archive",
        "checksumList": count === 1 && info.isDir === false && App.isChecksumList(info.uri || "")
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

        header: HeaderBand {
            PageHeader {
                title: page.count === 1 ? App.nameOf(page.info.uri) : qsTrId("lautta-dir-selected", page.count)
            }
        }

        delegate: MoreActionRow {
            text: modelData.text
            icon: modelData.icon
            onClicked: page.choose(modelData.id)
        }

        VerticalScrollDecorator { }
    }
}
