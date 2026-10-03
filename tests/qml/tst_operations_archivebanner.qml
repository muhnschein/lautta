// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates ArchiveBanner: hidden for a folder that is not in an archive.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/components"

ApplicationWindow {
    initialPage: Component {
        Page {
            ArchiveBanner {
                uri: "lautta://user-documents/"
                Component.onCompleted: {
                    if (visible)
                        console.error("the banner must be hidden outside archives")
                }
            }
        }
    }
}
