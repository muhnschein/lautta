// SPDX-FileCopyrightText: 2012-2023 Jolla Ltd.
// SPDX-FileCopyrightText: 2019 Open Mobile Platform LLC.
// SPDX-FileCopyrightText: 2025 Jolla Mobile Ltd
//
// SPDX-License-Identifier: BSD-3-Clause
//
// jolla-gallery's src/pages/FlickableImageView.qml, adapted for lautta
// (PRV-4, see LAUTTA.md): items are lautta URIs with a video flag (the
// model's `uri` and `video` roles, or objects of an array); images show
// through image://lautta-file/, videos in VideoPoster, which plays only the
// current one; the overlay's actions are the page's. A zoomed image takes
// the drags, so swipes wait until it is zoomed out again, and while a delete
// waits for its remorse (`held`); a playing video without its controls
// takes them as in the Gallery. Neighbours load once the current item has,
// so it gets the cores first.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../ViewerImage.js" as ViewerImage

PagedView {
    id: root

    property alias overlay: overlay
    property bool held
    // Opened image and size its opener showed its thumbnail at: that cached tile shows while it
    // decodes. Only that one: the thumbnailer makes a missing tile on Qt's one image thread.
    property string tileUri
    property int tileSize
    readonly property bool zoomed: currentItem !== null && currentItem.zoomed

    // What an image's Image item loads.
    function sourceOf(uri) {
        return ViewerImage.source(uri)
    }

    readonly property bool playing: currentItem !== null && currentItem.playing
    readonly property bool currentSettled: currentItem !== null && currentItem.item !== null && currentItem.item.settled

    interactive: count > 1 && !zoomed && !held && !(playing && !overlay.active)

    delegate: Loader {
        id: slot

        readonly property string itemUri: model.uri !== undefined ? model.uri : model.modelData.uri
        readonly property bool video: model.uri !== undefined ? model.video : model.modelData.video === true
        readonly property string mimeType: model.uri !== undefined ? model.mimeType : model.modelData.mimeType || ""
        readonly property url source: itemUri.length > 0 ? root.sourceOf(itemUri) : ""
        readonly property bool isCurrentItem: PagedView.isCurrentItem
        readonly property bool error: item && item.error
        readonly property bool zoomed: item !== null && item.zoomed
        readonly property bool playing: item !== null && item.playing === true
        readonly property QtObject player: video && item !== null ? item.player : null
        readonly property bool wanted: isCurrentItem || root.currentSettled
        property bool loadAllowed

        onWantedChanged: if (wanted) loadAllowed = true
        Component.onCompleted: if (wanted) loadAllowed = true

        width: root.width
        height: root.height
        sourceComponent: video ? videoComponent : imageComponent
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
                placeholder: slot.itemUri.length > 0 ? ViewerImage.placeholder(App, slot.itemUri) : ""
                tile: slot.itemUri === root.tileUri ? ViewerImage.tile(App, slot.itemUri, root.tileSize) : ""
                tileSize: root.tileSize
                active: slot.isCurrentItem
                viewMoving: root.moving
            }
        }

        Component {
            id: videoComponent

            VideoPoster {
                onClicked: overlay.active = !overlay.active
                onPlayingChanged: if (playing && overlay.active) overlay.active = false

                uri: slot.itemUri
                mimeType: slot.mimeType
                loadAllowed: slot.loadAllowed
                active: slot.isCurrentItem
                overlayMode: overlay.active
            }
        }
    }

    GalleryOverlay {
        id: overlay

        source: root.currentItem ? root.currentItem.source : ""
        error: root.currentItem !== null && root.currentItem.error
        held: root.held
        video: root.currentItem !== null && root.currentItem.video
        player: root.currentItem !== null ? root.currentItem.player : null

        anchors.fill: parent
        z: root.count + 100
    }
}
