// SPDX-FileCopyrightText: 2012-2023 Jolla Ltd.
// SPDX-FileCopyrightText: 2019 Open Mobile Platform LLC.
// SPDX-FileCopyrightText: 2025 Jolla Mobile Ltd
//
// SPDX-License-Identifier: BSD-3-Clause
//
// jolla-gallery's src/pages/FlickableImageView.qml, adapted for lautta
// (PRV-4, see LAUTTA.md): images only (the video player goes), items are
// lautta URIs (the model's `uri` role, or the items of an array) shown
// through image://lautta-file/, and the overlay's actions are the page's.
// A zoomed image takes the drags, so swipes wait until it is zoomed out
// again, and while a delete waits for its remorse (`held`). Neighbours load
// once the current image has, so it gets the cores first.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../ViewerImage.js" as ViewerImage

PagedView {
    id: root

    property alias overlay: overlay
    property bool held
    property int tileSize
    readonly property bool zoomed: currentItem !== null && currentItem.zoomed

    // What an image's Image item loads.
    function sourceOf(uri) {
        return ViewerImage.source(uri)
    }

    readonly property bool currentSettled: currentItem !== null && currentItem.item !== null
                                           && (currentItem.item.photo.status === Image.Ready
                                               || currentItem.item.photo.status === Image.Error)

    interactive: count > 1 && !zoomed && !held

    delegate: Loader {
        id: slot

        readonly property string imageUri: model.uri !== undefined ? model.uri : model.modelData
        readonly property url source: imageUri.length > 0 ? root.sourceOf(imageUri) : ""
        readonly property bool isCurrentItem: PagedView.isCurrentItem
        readonly property bool error: item && item.error
        readonly property bool zoomed: item !== null && item.zoomed
        readonly property bool wanted: isCurrentItem || root.currentSettled
        property bool loadAllowed

        onWantedChanged: if (wanted) loadAllowed = true
        Component.onCompleted: if (wanted) loadAllowed = true

        width: root.width
        height: root.height
        sourceComponent: imageComponent
        asynchronous: !isCurrentItem

        Component {
            id: imageComponent

            GalleryImage {
                onZoomedChanged: overlay.active = !zoomed
                onClicked: {
                    if (zoomed) {
                        zoomOut()
                    } else {
                        overlay.active = !overlay.active
                    }
                }

                source: slot.loadAllowed ? slot.source : ""
                placeholder: slot.imageUri.length > 0 ? ViewerImage.placeholder(App, slot.imageUri) : ""
                tile: slot.imageUri.length > 0 ? ViewerImage.tile(App, slot.imageUri, root.tileSize) : ""
                tileSize: root.tileSize
                active: slot.isCurrentItem
                viewMoving: root.moving
            }
        }
    }

    GalleryOverlay {
        id: overlay

        source: root.currentItem ? root.currentItem.source : ""
        error: root.currentItem !== null && root.currentItem.error
        held: root.held

        anchors.fill: parent
        z: root.count + 100
    }
}
