// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/dialogs"

ApplicationWindow {
    id: test
    property bool reachedEnd

    initialPage: Component {
        FolderPickerDialog {
            Component.onCompleted: test.reachedEnd = true
            title: "Copy 3 items to"
            acceptText: "Copy here"
            startUri: "lautta://user-documents/"
        }
    }
}
