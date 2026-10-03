// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    initialPage: Component {
        TagPage {
            tagId: 1
            listModel: ListModel {
                ListElement { uri: "lautta://user-documents/plan.odt"; name: "plan.odt"; place: "Documents › Work"; missing: false; section: "items"; isDir: false }
                ListElement { uri: "lautta://nv-account:1/work"; name: "Client contracts"; place: "NAS › /srv"; missing: false; section: "items"; isDir: true }
                ListElement { uri: "lautta://nv-account:2/budget-old.xlsx"; name: "budget-old.xlsx"; place: "Office › /finance"; missing: true; section: "missing"; isDir: false }
            }
        }
    }
}
