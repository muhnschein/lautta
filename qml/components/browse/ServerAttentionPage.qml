// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../directory"
import "../ServerWarning.js" as ServerWarning

Page {
    id: page

    property string locationId
    property string serverName
    property string attention

    readonly property string kind: ServerWarning.kindOf(attention)

    function act(action) {
        if (action === "account")
            Bridge.editAccount("lautta://" + page.locationId + "/")
        else
            pageStack.pop()
    }

    allowedOrientations: Orientation.All

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: Math.max(height, warning.y + warning.height + Theme.paddingLarge)

        PageHeader {
            id: header
            title: page.serverName
            description: App.displayAddress("lautta://" + page.locationId + "/")
        }

        FolderPlaceholder {
            id: warning

            objectName: "attentionWarning"
            x: Theme.horizontalPageMargin
            y: Math.max(header.height, Math.round((page.height - height) / 2))
            level: ServerWarning.level(page.kind)
            title: ServerWarning.title(page.kind)
            hint: ServerWarning.hint(page.kind, page.serverName)
            //% "Go back"
            buttons: [{ "text": ServerWarning.fixText(page.kind), "action": "account", "primary": true },
                      { "text": qsTrId("lautta-attention-back"), "action": "back" }]
            onTriggered: page.act(action)
        }

        VerticalScrollDecorator {}
    }
}
