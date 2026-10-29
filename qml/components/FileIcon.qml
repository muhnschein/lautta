// SPDX-License-Identifier: LGPL-2.1-or-later
// The icon of a file or folder (doc/QML-API.md): the app's own glyph per
// kind (FileIcons.js, UI-2), the system thumbnail for local images, videos
// and PDFs (Nemo.Thumbnailer, PRV-1: it draws a PDF or video only when given
// `mimeType`) and the app's thumbnail provider for remote images and PDFs
// (PRV-2, PRV-14). `thumbnailSource` is what DirectoryModel's role gives: a
// file:// URL, an image://lautta-thumb/ URL or empty. A thumbnail that cannot
// be made (an encrypted PDF) leaves the category's glyph, and
// `thumbnailFailed` says so. A shown video thumbnail keeps the video glyph at
// the size and place the plain glyph has, on a translucent dark disc, so it
// reads as a video, not as a photo (PRV-1). `previewText`, the grid's text
// thumbnail (PRV-15), takes the glyph's place: the file's first lines at
// caption size, faded out above its type icon.
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
    property string previewText
    property bool previewMono
    // An app icon name, qml/icons/types/.
    property string typeIcon
    readonly property bool previewShown: previewText.length > 0
    // A video's thumbnail shows: the glyph stays over it, light on a dark disc.
    readonly property bool videoShown: thumbnailReady && category === "video" && !isDir

    function iconFor(name, dir) {
        return FileIcons.glyph(name, dir, glyphSize)
    }

    width: size
    height: size

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

    // Behind the glyph over a video's thumbnail: dark whatever the ambience, as
    // Jolla Gallery's overlays, a little wider than the glyph's ring so the
    // frame shows around it; an even step in from the glyph's side keeps it
    // centred on whole pixels. Contrast on bright and dark frames (PRV-1);
    // translucent enough that the frame shows through.
    Rectangle {
        objectName: "videoBadge"

        anchors.centerIn: parent
        width: icon.glyphSize - 2 * Math.floor(icon.glyphSize / 10)
        height: width
        radius: width / 2
        color: Theme.rgba(Theme.highlightDimmerFromColor(Theme.highlightDimmerColor, Theme.LightOnDark),
                          Theme.opacityHigh)
        visible: icon.videoShown
    }

    // The category's glyph: alone when no thumbnail shows, else over a video's
    // (the one item, so the glyph is the plain one in size and place). Light
    // on the dark disc and a little translucent, its highlight colour for that
    // scheme (Jolla Gallery, VerticalPageBackHint).
    AppIcon {
        objectName: "fileGlyph"

        anchors.centerIn: parent
        icon: icon.iconFor(icon.category, icon.isDir)
        iconSize: icon.glyphSize
        color: icon.videoShown ? Theme.lightPrimaryColor : Theme.primaryColor
        highlightColor: icon.videoShown ? Theme.highlightFromColor(Theme.highlightColor, Theme.LightOnDark)
                                        : Theme.highlightColor
        highlighted: icon.highlighted
        opacity: icon.videoShown ? Theme.opacityOverlay : 1
        visible: !(icon.thumbnailReady || icon.previewShown) || icon.videoShown
    }

    Loader {
        anchors.fill: parent
        active: icon.previewShown
        sourceComponent: textThumbnail
    }

    Component {
        id: textThumbnail

        Item {
            Label {
                id: previewLabel

                objectName: "textPreview"
                anchors {
                    fill: parent
                    margins: Theme.paddingMedium
                }
                text: icon.previewText
                textFormat: Text.PlainText
                wrapMode: icon.previewMono ? Text.NoWrap : Text.Wrap
                clip: true
                font.pixelSize: Theme.fontSizeTiny
                font.family: icon.previewMono ? "monospace" : Theme.fontFamily
                color: icon.highlighted ? Theme.highlightColor : Theme.secondaryColor
            }
            // Solid to half its height, gone at 80 %: no text behind the type icon.
            OpacityRampEffect {
                anchors.fill: previewLabel
                sourceItem: previewLabel
                direction: OpacityRamp.TopToBottom
                offset: 0.5
                slope: 1 / 0.3
            }
            AppIcon {
                objectName: "typeIcon"

                anchors {
                    right: parent.right
                    bottom: parent.bottom
                    margins: Theme.paddingMedium
                }
                icon: icon.typeIcon
                iconSize: Math.round(Theme.iconSizeSmall * 2 / 3)
                highlighted: icon.highlighted
            }
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
