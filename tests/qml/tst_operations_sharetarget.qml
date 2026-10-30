// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/pages"

ApplicationWindow {
    initialPage: Component {
        ShareTargetPage {
            id: share

            resources: [ "file:///home/user/Downloads/IMG_2041.jpg", { "filePath": "/home/user/Documents/b.txt" } ]
            Component.onCompleted: {
                if (entries.length !== 2 || readableCount !== 0 || skippedNames.length !== 2)
                    console.error("unreadable files are skipped: " + readableCount + "/" + skippedNames.length)
                if (canAccept)
                    console.error("nothing readable, nothing to save")
                if (note !== "2 files can't be read, skipped")
                    console.error("the note names the skipped files: " + note)
                var view = share.children.filter(function (c) { return c.objectName === "shareFolder" })[0]
                if (!view || !view.picking)
                    console.error("the body is a folder view in picking mode")
                destUri = ""
                if (view.visible)
                    console.error("Locations replace the folder view")
            }
        }
    }
}
