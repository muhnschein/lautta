// SPDX-License-Identifier: LGPL-2.1-or-later
// Photo without camera thumbnail: placeholder = clear image of its upright shape, grid tile (system
// thumbnailer, square crop) under it, centred, side = shorter side the photo shows at; both go once
// the photo shows. Camera thumbnail drawn over the tile. Tile URL percent-encoded, none for size 0
// or an image not on the device. Viewer shows the opener's tile for the opened photo only.
import QtQuick 2.6
import Sailfish.Silica 1.0
// Registers image://nemoThumbnail, as the folder grid's thumbnails do in the app.
import Nemo.Thumbnailer 1.0
import Lautta 1.0
import "../../qml/viewers"
import "../../qml/components/viewers"
import "../../qml/components/viewers/gallery"
import "../../qml/components/viewers/ViewerImage.js" as ViewerImage

ApplicationWindow {
    id: window

    property bool reachedEnd
    readonly property string folder: "lautta://user-pictures/ViewerTile/"

    ViewerTools { id: tools }
    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
    }

    Component {
        id: galleryMaker
        GalleryImage { width: 540; height: 960 }
    }

    Component {
        id: viewerMaker
        ImageViewer {}
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

    function named(item, name) {
        if (!item)
            return null
        if (item.objectName === name)
            return item
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i) {
            var found = named(kids[i], name)
            if (found)
                return found
        }
        return item.contentItem && item.contentItem !== item ? named(item.contentItem, name) : null
    }

    function gallery(name, size) {
        return galleryMaker.createObject(pageStack.currentPage, {
            "placeholder": ViewerImage.placeholder(App, folder + name),
            "tile": ViewerImage.tile(App, folder + name, size),
            "tileSize": size
        })
    }

    function describe(item) {
        return item.x + "," + item.y + " " + item.width + "x" + item.height + " visible " + item.visible
    }

    function checkTileUrl() {
        var local = App.localUrl(folder + "a%20b%23c.jpg")
        expect(ViewerImage.tile(App, folder + "a%20b%23c.jpg", 170)
               === "image://nemoThumbnail/" + local.replace(" ", "%20").replace("#", "%23"),
               "tile URL percent-encoded: " + ViewerImage.tile(App, folder + "a%20b%23c.jpg", 170))
        expect(ViewerImage.tile(App, folder + "a.jpg", 0) === "", "no tile size: no tile")
        expect(ViewerImage.tile(App, "lautta://nosuch/a.jpg", 170) === "", "image not on the device: no tile")
    }

    // photo.jpg: 64x32, EXIF turned, upright 32x64. Name with space and '#' loads.
    function checkPortrait() {
        var g = gallery("a%20b%23c.jpg", 170)
        var shape = named(g, "placeholderImage")
        var tile = named(g, "tileImage")
        expect(tile.status === Image.Loading, "tile loads off the GUI thread: status " + tile.status)
        waitFor(function () { return shape.status === Image.Ready && tile.status === Image.Ready }, "the portrait's tile")
        expect(shape.implicitWidth === 32 && shape.implicitHeight === 64,
               "placeholder: the upright shape, " + shape.implicitWidth + "x" + shape.implicitHeight)
        expect(tile.sourceSize.width === 170 && tile.sourceSize.height === 170,
               "tile asked at the grid's size: " + tile.sourceSize.width + "x" + tile.sourceSize.height)
        expect(tile.visible && tile.x === 30 && tile.y === 240 && tile.width === 480 && tile.height === 480,
               "portrait's tile: its middle, " + describe(tile))
        g.source = ViewerImage.source(folder + "a%20b%23c.jpg")
        waitFor(function () { return g.photo.status === Image.Ready && g.photo.opacity === 1 }, "the photo")
        expect(!tile.visible && !shape.visible, "photo shown: tile goes, " + describe(tile))
        g.destroy()
    }

    // large-photo.jpg: 1280x960.
    function checkLandscape() {
        var g = gallery("wide.jpg", 340)
        var shape = named(g, "placeholderImage")
        var tile = named(g, "tileImage")
        waitFor(function () { return shape.status === Image.Ready && tile.status === Image.Ready }, "the landscape's tile")
        // Centring snaps to whole pixels.
        expect(tile.visible && Math.abs(tile.x - 67.5) <= 0.5 && Math.abs(tile.y - 277.5) <= 0.5
               && tile.width === 405 && tile.height === 405,
               "landscape's tile: its middle, " + describe(tile))
        g.destroy()
    }

    function checkCameraOver() {
        var g = gallery("cam.jpg", 170)
        var thumb = named(g, "placeholderImage")
        var tile = named(g, "tileImage")
        waitFor(function () { return thumb.status === Image.Ready && tile.status === Image.Ready }, "the camera photo's tile")
        expect(thumb.implicitWidth === 80 && tile.z < thumb.z, "camera thumbnail over the tile: z " + tile.z + " under " + thumb.z)
        g.destroy()
    }

    function allTiles(item, out) {
        if (item.objectName === "tileImage" && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            allTiles(kids[i], out)
        if (item.contentItem && item.contentItem !== item)
            allTiles(item.contentItem, out)
        return out
    }

    function shownTile(viewer) {
        var slot = viewer.slideshow ? viewer.slideshow.currentItem : null
        return slot && slot.item ? named(slot.item, "tileImage") : null
    }

    // Photo alone until the folder's listing comes, then the folder's view: both have the tile.
    function checkViewerHandsSize() {
        // As a url: Qt lowercases the provider's name.
        var want = String(Qt.resolvedUrl(ViewerImage.tile(App, folder + "a.jpg", 170)))
        var viewer = pageStack.push(viewerMaker, { "uri": folder + "a.jpg", "folderUri": folder, "tileSize": 170 },
                                    PageStackAction.Immediate)
        var tile = shownTile(viewer)
        expect(!viewer.inFolder && tile !== null && String(tile.source) === want && tile.sourceSize.width === 170,
               "photo alone: the opener's tile, " + (tile ? tile.source + " at " + tile.sourceSize.width : "none"))
        waitFor(function () { return viewer.inFolder }, "the folder's listing")
        waitFor(function () { tile = shownTile(viewer); return tile !== null && tile.status === Image.Ready }, "the folder view's tile")
        expect(String(tile.source) === want && tile.sourceSize.width === 170,
               "folder's view: the opener's tile, " + tile.source + " at " + tile.sourceSize.width)
        var asked = allTiles(viewer.slideshow, []).filter(function (t) { return String(t.source) !== "" })
        expect(allTiles(viewer.slideshow, []).length > 1 && asked.length === 1,
               "neighbours ask no tile: " + asked.map(function (t) { return t.source }).join(" "))
        pageStack.pop(undefined, PageStackAction.Immediate)
    }

    Component.onCompleted: {
        pictures.createFolder("ViewerTile")
        waitFor(function () { return pictures.indexOf(folder.slice(0, -1)) >= 0 }, "the folder")
        if (!expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "a.jpg")
                    && tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "a%20b%23c.jpg")
                    && tools.installFixture(Qt.resolvedUrl("host/fixtures/large-photo.jpg"), folder + "wide.jpg")
                    && tools.installFixture(Qt.resolvedUrl("host/fixtures/camera.jpg"), folder + "cam.jpg"),
                    "fixtures installed"))
            return
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        checkTileUrl()
        checkPortrait()
        checkLandscape()
        checkCameraOver()
        checkViewerHandsSize()
        reachedEnd = true
    }

    Component {
        id: pageComponent
        Page {}
    }
}
