// SPDX-License-Identifier: LGPL-2.1-or-later Nemo.Thumbnailer draws a PDF or video only when given
// `mimeType`.
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
    property real glyphSize: Theme.iconSizeMedium
    property bool highlighted
    readonly property bool thumbnailReady: thumbnailLoader.item ? thumbnailLoader.item.ready : false
    readonly property bool thumbnailFailed: thumbnailLoader.item ? thumbnailLoader.item.failed : false
    property string previewText
    property bool previewMono
    property string typeIcon
    readonly property bool previewShown: previewText.length > 0
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
            id: textTile

            // `icon` inside AppIcon is its own property, not this file's id.
            readonly property Item fileIcon: icon

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
                icon: textTile.fileIcon.typeIcon
                iconSize: Math.round(Theme.iconSizeSmall * 2 / 3)
                highlighted: textTile.fileIcon.highlighted
            }
        }
    }

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
