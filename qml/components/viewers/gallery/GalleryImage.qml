// SPDX-FileCopyrightText: 2013-2023 Jolla Ltd.
// SPDX-FileCopyrightText: 2020 Open Mobile Platform LLC.
// SPDX-FileCopyrightText: 2024-2025 Jolla Mobile Ltd
//
// SPDX-License-Identifier: BSD-3-Clause
//
// sailfish-components-gallery's src/ImageViewer.qml, adapted for lautta
// (PRV-4, see LAUTTA.md): it zooms with lautta's ZoomableFlickable, which
// also has the tap area (and a double tap), the image turns itself upright
// (autoTransform; remote images come upright from image://lautta-file/)
// instead of through ImageMetadata, and the error text is lautta's.
import QtQuick 2.6
import Sailfish.Silica 1.0
import ".."

ZoomableFlickable {
    id: flickable

    property alias source: photo.source

    property bool active: true
    readonly property bool _active: active || viewMoving
    readonly property bool error: photo.status == Image.Error

    property alias photo: photo
    property alias largePhoto: largePhoto

    onAboutToZoom: {
        if (largePhoto.source != photo.source) {
            largePhoto.source = photo.source
        }
    }

    scrollDecoratorColor: Theme.lightPrimaryColor

    zoomEnabled: photo.status == Image.Ready
    maximumZoom: Math.max(Screen.width, Screen.height) / 200
                 * Math.max(1.0, photo.implicitWidth > 0 ? largePhoto.implicitHeight / photo.implicitHeight
                                                         : 1.0)

    on_ActiveChanged: {
        if (!_active) {
            resetZoom()
            largePhoto.source = ""
        }
    }

    implicitContentWidth: photo.implicitWidth
    implicitContentHeight: photo.implicitHeight

    Image {
        id: photo

        property var errorLabel
        objectName: "zoomableImage"

        anchors.fill: parent
        smooth: !(movingVertically || movingHorizontally)
        sourceSize.width: Screen.height
        fillMode: Image.PreserveAspectFit
        visible: largePhoto.status !== Image.Ready
        asynchronous: true
        autoTransform: true
        cache: false

        onStatusChanged: {
            if (status == Image.Error) {
                errorLabel = errorLabelComponent.createObject(flickable)
            }
        }

        onSourceChanged: {
            if (errorLabel) {
                errorLabel.destroy()
                errorLabel = null
            }

            resetZoom()
        }

        opacity: status == Image.Ready ? 1 : 0
        Behavior on opacity { FadeAnimation{} }
    }

    Image {
        id: largePhoto

        sourceSize {
            width: 3264
            height: 3264
        }
        cache: false
        asynchronous: true
        autoTransform: true
        anchors.fill: parent
    }

    BusyIndicator {
        running: photo.status === Image.Loading && !delayBusyIndicator.running
        size: BusyIndicatorSize.Large
        anchors.centerIn: parent
        parent: flickable

        Timer {
            id: delayBusyIndicator

            running: photo.status === Image.Loading
            interval: 1000
        }
    }

    Component {
        id: errorLabelComponent

        InfoLabel {
            //% "Can't show this image"
            text: qsTrId("lautta-viewers-image-error")
            anchors.verticalCenter: parent.verticalCenter
            opacity: photo.status == Image.Error ? 1.0 : 0.0
            Behavior on opacity { FadeAnimator {}}
        }
    }
}
