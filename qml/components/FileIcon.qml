// SPDX-License-Identifier: LGPL-2.1-or-later
// The icon of a file or folder (doc/QML-API.md): the app's own glyph per
// kind (FileIcons.js, UI-2), the system thumbnail for local images, videos
// and PDFs (Nemo.Thumbnailer, PRV-1: it draws a PDF or video only when given
// `mimeType`) and the app's thumbnail provider for remote images and PDFs
// (PRV-2, PRV-14). `thumbnailSource` is what DirectoryModel's role gives: a
// file:// URL, an image://lautta-thumb/ URL or empty. A thumbnail that cannot
// be made (an encrypted PDF) leaves the category's glyph, and
// `thumbnailFailed` says so.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0
import "FileIcons.js" as FileIcons

Item {
    id: icon

    property string category: "file"
    property bool isDir
    property string thumbnailSource
    property bool isSymlink
    property bool selected
    property string mimeType
    property real size: Theme.iconSizeMedium
    // The side of the glyph inside; the grid draws it large.
    property real glyphSize: Theme.iconSizeMedium
    // Glyphs take the highlight colour: the row or tile is pressed or selected.
    property bool highlighted
    readonly property bool thumbnailReady: thumbnailLoader.item ? thumbnailLoader.item.ready : false
    readonly property bool thumbnailFailed: thumbnailLoader.item ? thumbnailLoader.item.failed : false

    function iconFor(name, dir) {
        return FileIcons.glyph(name, dir, glyphSize)
    }

    width: size
    height: size

    AppIcon {
        objectName: "fileGlyph"

        anchors.centerIn: parent
        icon: icon.iconFor(icon.category, icon.isDir)
        iconSize: icon.glyphSize
        highlighted: icon.highlighted
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
            readonly property bool failed: status === Thumbnail.Error

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
            readonly property bool failed: status === Image.Error

            source: icon.thumbnailSource + "?size=" + Math.round(icon.size)
            sourceSize.width: icon.width
            sourceSize.height: icon.height
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            visible: ready
        }
    }

    // A video's thumbnail keeps the video glyph at its bottom left, on a dark
    // gradient as Jolla Gallery's video thumbnails: else it reads as a
    // picture. Not on the plain glyph, which is the video glyph already.
    Item {
        objectName: "videoBadge"

        anchors.fill: parent
        visible: icon.thumbnailReady && icon.category === "video" && !icon.isDir

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: parent.height / 2
            opacity: Theme.opacityOverlay
            gradient: Gradient {
                GradientStop { position: 0.0; color: "transparent" }
                GradientStop { position: 1.0; color: Theme.highlightDimmerColor }
            }
        }

        AppIcon {
            objectName: "videoBadgeGlyph"

            anchors {
                left: parent.left
                bottom: parent.bottom
                margins: Theme.paddingSmall
            }
            // Small beside its thumbnail: up to three eighths of a list row's,
            // up to the small icon size on a grid tile, at a sharp size.
            iconSize: FileIcons.sharpAtMost(Math.min(Theme.iconSizeSmall, icon.size * 3 / 8))
            icon: FileIcons.glyph("video", false, iconSize)
            highlighted: icon.highlighted
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
