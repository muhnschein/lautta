// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates CompressDialog for two files.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/dialogs"

ApplicationWindow {
    initialPage: Component {
        CompressDialog {
            uris: [ "lautta://user-documents/a.txt", "lautta://user-documents/b.txt" ]
            Component.onCompleted: {
                if (destUri !== "lautta://user-documents/")
                    console.error("the archive goes next to the items by default: " + destUri)
            }
        }
    }
}
