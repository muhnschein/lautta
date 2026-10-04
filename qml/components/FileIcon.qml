// SPDX-License-Identifier: LGPL-2.1-or-later
// The icon of a file or folder (doc/QML-API.md): a theme icon per category,
// the system thumbnail for local images and videos (Nemo.Thumbnailer, PRV-1)
// and the app's thumbnail provider for remote ones (PRV-2). `thumbnailSource`
// is what DirectoryModel's role gives: a file:// URL, an image://lautta-thumb/
// URL or empty.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0

Item {
    id: icon

    property string category: "file"
    property bool isDir
    property string thumbnailSource
    property bool isSymlink
    property bool selected
    property string mimeType
    property real size: Theme.iconSizeMedium
    readonly property bool thumbnailReady: thumbnailLoader.item ? thumbnailLoader.item.ready : false

    function iconFor(name, dir) {
        if (dir)
            return "image://theme/icon-m-file-folder"
        switch (name) {
        case "folder": return "image://theme/icon-m-file-folder"
        case "image": return "image://theme/icon-m-file-image"
        case "video": return "image://theme/icon-m-file-video"
        case "audio": return "image://theme/icon-m-file-audio"
        case "archive": return "image://theme/icon-m-file-compressed"
        case "package": return "image://theme/icon-m-file-rpm"
        case "text":
        case "code":
        case "markdown":
        case "pdf":
        case "document":
        case "spreadsheet":
        case "presentation":
            return "image://theme/icon-m-document"
        case "database": return "dir-database"
        default: return "image://theme/icon-m-document"
        }
    }

    width: size
    height: size

    AppIcon {
        anchors.centerIn: parent
        icon: icon.iconFor(icon.category, icon.isDir)
        visible: !icon.thumbnailReady
    }

    Loader {
        id: thumbnailLoader

        anchors.fill: parent
        active: icon.thumbnailSource.length > 0
        sourceComponent: icon.thumbnailSource.indexOf("file://") === 0 ? localThumbnail : remoteThumbnail
    }

    Component {
        id: localThumbnail

        Thumbnail {
            readonly property bool ready: status === Thumbnail.Ready

            source: icon.thumbnailSource
            mimeType: icon.mimeType
            sourceSize.width: icon.width
            sourceSize.height: icon.height
            fillMode: Thumbnail.PreserveAspectCrop
            visible: ready
        }
    }

    Component {
        id: remoteThumbnail

        Image {
            readonly property bool ready: status === Image.Ready

            source: icon.thumbnailSource + "?size=" + Math.round(icon.size)
            sourceSize.width: icon.width
            sourceSize.height: icon.height
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            visible: ready
        }
    }

    // A link badge on symlinks (board Directory, "Old drafts").
    Rectangle {
        visible: icon.isSymlink
        anchors {
            right: parent.right
            bottom: parent.bottom
            margins: -Theme.paddingSmall / 2
        }
        width: Theme.iconSizeSmall * 0.75
        height: width
        radius: width / 2
        color: Theme.rgba(Theme.overlayBackgroundColor, 0.9)

        AppIcon {
            anchors.centerIn: parent
            small: true
            icon: "image://theme/icon-m-link"
            width: parent.width * 0.8
            height: width
            sourceSize.width: width
            sourceSize.height: width
        }
    }

    // Selected items get a frame (design: DirectorySelect).
    Rectangle {
        visible: icon.selected
        anchors {
            fill: parent
            margins: -Theme.paddingSmall / 3
        }
        color: "transparent"
        border {
            width: Math.max(2, Theme.paddingSmall / 3)
            color: Theme.highlightColor
        }
    }
}
