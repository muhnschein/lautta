// SPDX-License-Identifier: LGPL-2.1-or-later
// FileIcon in every shape and the path menu, open and editing.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"

ApplicationWindow {
    PathModel {
        id: path
        uri: "lautta://user-documents/Uni"
    }

    initialPage: Component {
        Page {
            Column {
                width: parent.width

                Row {
                    FileIcon { isDir: true; category: "folder" }
                    FileIcon { isDir: true; category: "folder"; isSymlink: true }
                    FileIcon { category: "image"; selected: true }
                    FileIcon { category: "video" }
                    FileIcon { category: "audio" }
                    FileIcon { category: "archive" }
                    FileIcon { category: "text" }
                    FileIcon { category: "database" }
                    FileIcon { category: "other" }
                    FileIcon { category: "image"; thumbnailSource: "file:///nonexistent.png"; mimeType: "image/png" }
                    FileIcon { category: "image"; thumbnailSource: "image://lautta-thumb/lautta://nv-x/a.png" }
                }
                PathMenu {
                    open: true
                    editing: true
                    pathModel: path
                    currentUri: "lautta://user-documents/Uni"
                }
            }
        }
    }
}
