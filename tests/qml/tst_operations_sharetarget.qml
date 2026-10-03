// SPDX-License-Identifier: LGPL-2.1-or-later
// Instantiates ShareTargetPage with a file URL and a resource object; neither
// file exists in the test home, so both are reported as unreadable.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/pages"

ApplicationWindow {
    initialPage: Component {
        ShareTargetPage {
            resources: [ "file:///home/user/Downloads/IMG_2041.jpg", { "filePath": "/home/user/Documents/b.txt" } ]
            Component.onCompleted: {
                if (entries.length !== 2 || readableCount !== 0 || skippedCount !== 2)
                    console.error("unreadable files are skipped: " + readableCount + "/" + skippedCount)
                if (canAccept)
                    console.error("nothing readable, nothing to save")
            }
        }
    }
}
