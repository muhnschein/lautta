// SPDX-FileCopyrightText: 2017-2023 Jolla Ltd.
// SPDX-FileCopyrightText: 2024-2025 Jolla Mobile Ltd
//
// SPDX-License-Identifier: BSD-3-Clause
//
// sailfish-components-gallery's src/GalleryOverlay.qml, adapted for lautta
// (PRV-4, see LAUTTA.md): images only (the video controls, the image editor
// and Ambience go), the close button of Silica's DismissButton top right,
// and lautta's actions as signals for the page: details (top left, for
// every location), and in the toolbar Delete, Open with (where the
// Gallery's Edit is), Share and More. A delete waiting for its remorse
// (`held`) puts the controls away, as there.
import QtQuick 2.6
import Sailfish.Silica 1.0
import ".."

Item {
    id: overlay

    property bool active: true
    // A delete is waiting for its remorse.
    property bool held

    property alias toolbar: toolbar
    property alias detailsButton: detailsButton
    property alias topFade: topFade
    property alias fadeOpacity: topFade.fadeOpacity

    property url source
    property bool error

    signal details
    signal remove
    signal openWith
    signal share
    signal more

    enabled: active && source != "" && !held
    Behavior on opacity { FadeAnimator {}}
    opacity: enabled ? 1.0 : 0.0

    FadeGradient {
        id: topFade

        width: parent.width
        height: detailsButton.height + 2 * detailsButton.y
        topDown: true
    }

    FadeGradient {
        id: bottomFade

        fadeOpacity: topFade.fadeOpacity
        width: parent.width
        height: toolbar.height + 2 * toolbarParent.anchors.bottomMargin
        anchors.bottom: parent.bottom
    }

    IconButton {
        id: detailsButton

        objectName: "detailsButton"
        x: Theme.horizontalPageMargin
        y: Theme.paddingLarge
        icon.source: "image://theme/icon-m-about"
        onClicked: overlay.details()
    }

    IconButton {
        id: dismissButton

        objectName: "dismissButton"
        y: Theme.paddingLarge
        icon.source: "image://theme/icon-m-dismiss"
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        onClicked: pageStack.pop()
    }

    Item {
        id: toolbarParent

        width: parent.width
        anchors {
            bottom: parent.bottom
            bottomMargin: parent.height/20
        }
        height: toolbar.height

        Row {
            id: toolbar

            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Theme.paddingLarge

            IconButton {
                id: deleteButton

                objectName: "deleteButton"
                icon.source: "image://theme/icon-m-delete"
                anchors.verticalCenter: parent.verticalCenter
                onClicked: overlay.remove()
            }

            IconButton {
                id: openButton

                objectName: "openButton"
                icon.source: "image://theme/icon-m-edit"
                visible: !overlay.error
                anchors.verticalCenter: parent.verticalCenter
                onClicked: overlay.openWith()
            }

            IconButton {
                id: shareButton

                objectName: "shareButton"
                icon.source: "image://theme/icon-m-share"
                visible: !overlay.error
                anchors.verticalCenter: parent.verticalCenter
                onClicked: overlay.share()
            }

            IconButton {
                id: moreButton

                objectName: "moreButton"
                icon.source: "image://theme/icon-m-other"
                anchors.verticalCenter: parent.verticalCenter
                onClicked: overlay.more()
            }
        }
    }
}
