// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    DirectoryModel {
        id: folder
        uri: "lautta://user-documents/"
    }

    initialPage: Component {
        ViewOptionsPage {
            model: folder
        }
    }
}
