// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    initialPage: Component {
        RecentsPage {
            listModel: ListModel {
                ListElement { itemId: 1; uri: "lautta://nv-account:1/srv/photos/IMG_2041.jpg"; name: "IMG_2041.jpg"; kind: "opened"; place: "NAS › /srv/photos"; at: 1; day: "today" }
                ListElement { itemId: 2; uri: "lautta://user-documents/notes.txt"; name: "notes.txt"; kind: "edited"; place: "Documents"; at: 2; day: "today" }
                ListElement { itemId: 3; uri: "lautta://nv-account:1/invoice-0912.pdf"; name: "invoice-0912.pdf"; kind: "transferred"; place: "NAS"; at: 3; day: "yesterday" }
                ListElement { itemId: 4; uri: "lautta://user-documents/README.md"; name: "README.md"; kind: "previewed"; place: "Documents"; at: 4; day: "week" }
                ListElement { itemId: 5; uri: "lautta://user-documents/old.csv"; name: "old.csv"; kind: "opened"; place: ""; at: 5; day: "earlier" }
            }
        }
    }
}
