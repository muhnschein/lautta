// SPDX-License-Identifier: LGPL-2.1-or-later
// Asks before a delete that skips the Trash (OPS-8, board ImageDeleteRemote):
// on volumes and servers, and anywhere while the Trash is switched off. It
// names the item (or the count) and where it is. The dialog only asks: the
// caller deletes when it is accepted, with no remorse timer after it.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Dialog {
    id: dialog

    // The URIs of the items to delete.
    property var uris: []

    readonly property int count: uris ? uris.length : 0
    readonly property string place: count > 0 ? App.locationName(uris[0]) : ""

    function message() {
        if (count === 1)
            //% "“%1” will be deleted from %2. This can't be undone."
            return qsTrId("lautta-delete-permanent-one").arg(App.nameOf(uris[0])).arg(place)
        //% "%n items will be deleted from %1. This can't be undone."
        return qsTrId("lautta-delete-permanent-many", count).arg(place)
    }

    allowedOrientations: Orientation.All
    canAccept: count > 0

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column

            width: parent.width
            spacing: Theme.paddingLarge

            DialogHeader {
                //% "Delete"
                acceptText: qsTrId("lautta-delete-permanent-accept")
                //% "Delete permanently?"
                title: qsTrId("lautta-delete-permanent-title")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                text: dialog.message()
            }
        }
    }
}
