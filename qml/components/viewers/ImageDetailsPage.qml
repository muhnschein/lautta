// SPDX-License-Identifier: LGPL-2.1-or-later
// The details panel of the image viewer (PRV-4, board ImageExif): the image
// dimmed at the top (tap to go back), EXIF fields below.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "ViewerText.js" as ViewerText

Page {
    id: page

    property string uri
    // The source the viewer shows for this image.
    property url imageSource

    readonly property var info: exif.infoJson.length > 0 ? JSON.parse(exif.infoJson) : ({})

    allowedOrientations: Orientation.All

    ExifModel {
        id: exif
        uri: page.uri
    }

    Image {
        id: preview
        anchors { top: parent.top; left: parent.left; right: parent.right }
        height: Math.min(parent.height * 0.35, Theme.itemSizeHuge * 2)
        source: page.imageSource
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        autoTransform: true
        sourceSize.width: page.width
        sourceSize.height: page.width

        Rectangle {
            anchors.fill: parent
            color: Theme.overlayBackgroundColor
            opacity: Theme.opacityOverlay
        }
        MouseArea {
            anchors.fill: parent
            onClicked: pageStack.pop()
        }
    }

    SilicaFlickable {
        anchors { top: preview.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        contentHeight: column.height + Theme.paddingLarge
        clip: true

        Column {
            id: column
            width: parent.width

            PageHeader {
                //% "Details"
                title: qsTrId("lautta-viewers-details")
                description: App.nameOf(page.uri)
            }

            DetailItem {
                //% "Camera"
                label: qsTrId("lautta-viewers-exif-camera")
                value: page.info.model ? ViewerText.camera(page.info) : ""
                visible: value.length > 0
            }
            DetailItem {
                //% "Lens"
                label: qsTrId("lautta-viewers-exif-lens")
                value: page.info.lens ? page.info.lens
                                      : (page.info.focalLengthMm ? Number(page.info.focalLengthMm).toFixed(1) + " mm" : "")
                visible: value.length > 0
            }
            DetailItem {
                //% "Exposure"
                label: qsTrId("lautta-viewers-exif-exposure")
                value: ViewerText.exposureLine(page.info)
                visible: value.length > 0
            }
            DetailItem {
                //% "Taken"
                label: qsTrId("lautta-viewers-exif-taken")
                value: page.info.dateTaken ? page.info.dateTaken : ""
                visible: value.length > 0
            }
            DetailItem {
                //% "Dimensions"
                label: qsTrId("lautta-viewers-exif-dimensions")
                value: page.info.width && page.info.height ? page.info.width + " × " + page.info.height : ""
                visible: value.length > 0
            }
            DetailItem {
                //% "Size"
                label: qsTrId("lautta-viewers-exif-size")
                value: page.info.size >= 0 && page.info.size !== null && page.info.size !== undefined
                       ? Format.formatFileSize(page.info.size) : ""
                visible: value.length > 0
            }
            DetailItem {
                //% "Location"
                label: qsTrId("lautta-viewers-exif-location")
                value: page.info.latitude !== null && page.info.latitude !== undefined
                       ? ViewerText.coordinates(page.info.latitude, page.info.longitude) : ""
                visible: value.length > 0
            }
            DetailItem {
                //% "Orientation"
                label: qsTrId("lautta-viewers-exif-orientation")
                value: page.info.orientation ? ViewerText.orientation(page.info.orientation) : ""
                visible: value.length > 0
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: !exif.loading && exif.infoJson.length > 0 && !exif.hasExif
                wrapMode: Text.Wrap
                color: Theme.secondaryHighlightColor
                //% "This image has no camera details."
                text: qsTrId("lautta-viewers-exif-none")
            }
        }

        VerticalScrollDecorator { }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Medium
        running: exif.loading
    }
}
