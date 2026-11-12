// SPDX-License-Identifier: LGPL-2.1-or-later What to add to a folder. One switch at a time,
// nothing preset; picked page takes this dialog's place.
import QtQuick 2.6
import Sailfish.Silica 1.0

Dialog {
    id: dialog

    property string parentUri
    // "folder", "scan", or "" until picked.
    property string choice

    // Destination page once accepted: caller wires it up.
    signal picked(string choice, var page)

    function choose(which) {
        choice = choice === which ? "" : which
    }

    allowedOrientations: Orientation.All
    canAccept: choice.length > 0
    acceptDestination: choice === "folder" ? Qt.resolvedUrl("NewItemDialog.qml")
                     : choice === "scan" ? Qt.resolvedUrl("../pages/ScanPage.qml")
                     : ""
    acceptDestinationAction: PageStackAction.Replace
    // Only what both pages have: Silica may build the page before a pick-bound value follows.
    acceptDestinationProperties: ({ "parentUri": dialog.parentUri })

    onAccepted: picked(choice, acceptDestinationInstance)

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column

            width: parent.width

            DialogHeader {
                //% "Next"
                acceptText: qsTrId("lautta-add-next")
            }

            TextSwitch {
                objectName: "folderSwitch"
                //% "New folder"
                text: qsTrId("lautta-add-folder")
                //% "An empty folder in this place."
                description: qsTrId("lautta-add-folder-description")
                automaticCheck: false
                checked: dialog.choice === "folder"
                onClicked: dialog.choose("folder")
            }

            TextSwitch {
                objectName: "scanSwitch"
                //% "Scan document"
                text: qsTrId("lautta-add-scan")
                //% "Take pictures of pages with the camera. They are saved as one PDF."
                description: qsTrId("lautta-add-scan-description")
                automaticCheck: false
                checked: dialog.choice === "scan"
                onClicked: dialog.choose("scan")
            }
        }
    }
}
