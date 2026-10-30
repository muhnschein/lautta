// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    function find(item, name) {
        if (!item || item.objectName === name)
            return item
        for (var i = 0; i < item.children.length; ++i) {
            var hit = find(item.children[i], name)
            if (hit)
                return hit
        }
        return null
    }

    initialPage: Component {
        ShareTargetPage {
            id: share

            resources: [ "file:///home/user/Downloads/IMG_2041.jpg", { "filePath": "/home/user/Documents/b.txt" } ]
            Component.onCompleted: {
                if (entries.length !== 2 || readableCount !== 0 || skippedNames.length !== 2)
                    console.error("unreadable files are skipped: " + readableCount + "/" + skippedNames.length)
                if (canAccept)
                    console.error("nothing readable, nothing to save")
                if (note !== qsTrId("lautta-share-skipped-many", 2))
                    console.error("the note names the skipped files: " + note)
                var view = win.find(share, "shareFolder")
                if (!view || !view.picking)
                    return console.error("the body is a folder view in picking mode")
                destUri = ""
                if (view.visible)
                    console.error("Locations replace the folder view")
            }
        }
    }
}
