// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates BulkRenamePage for two files.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/pages"

ApplicationWindow {
    initialPage: Component {
        BulkRenamePage {
            uris: [ "lautta://user-documents/IMG_2041.jpg", "lautta://user-documents/IMG_2042.jpg" ]
        }
    }
}
