// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates ExtractDialog for an archive in Documents.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/dialogs"

ApplicationWindow {
    initialPage: Component {
        ExtractDialog {
            archiveUri: "lautta://user-documents/dataset.tar.gz"
            Component.onCompleted: {
                if (destUri !== "lautta://user-documents/")
                    console.error("extracting goes next to the archive by default: " + destUri)
            }
        }
    }
}
