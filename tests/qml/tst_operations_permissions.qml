// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates PermissionsPage for the Documents folder.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/pages"

ApplicationWindow {
    initialPage: Component {
        PermissionsPage {
            uri: "lautta://user-documents/"
        }
    }
}
