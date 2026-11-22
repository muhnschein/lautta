// SPDX-License-Identifier: LGPL-2.1-or-later
// ViewerPrefetch decodes an image for the viewer, then lets go; viewer's photo made afterwards is
// ready at once (Qt's pixmap cache, same request). Photo not fetched first still loads. Not for
// other files; `stop()` drops a running one. Placeholder = camera's EXIF thumbnail, none without.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/viewers"
import "../../qml/components/viewers/gallery"
import "../../qml/components/viewers/ViewerImage.js" as ViewerImage

ApplicationWindow {
    id: window

    property bool reachedEnd
    property var log: []
    readonly property string folder: "lautta://user-pictures/ViewerPrefetch/"

    ViewerTools { id: tools }
    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
    }

    ViewerPrefetch { id: prefetch }

    // Status as the source changes: Ready when let go after the decode, else stopped early.
    Connections {
        target: prefetch
        onSourceChanged: window.log.push({ "source": String(prefetch.source), "status": prefetch.status })
    }

    Component {
        id: galleryMaker
        GalleryImage { width: 540; height: 960 }
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

    function gallery(props) {
        return galleryMaker.createObject(pageStack.currentPage, props)
    }

    function checkOtherFilesNotFetched() {
        prefetch.start(folder + "notes.md", "text/markdown")
        expect(String(prefetch.source) === "" && log.length === 0, "a text file starts no decode: " + JSON.stringify(log))
    }

    function checkPrefetchFeedsViewer() {
        log = []
        prefetch.start(folder + "a.jpg", "image/jpeg")
        expect(String(prefetch.source) === ViewerImage.source(folder + "a.jpg"), "an image starts the viewer's request: " + prefetch.source)
        waitFor(function () { return String(prefetch.source) === "" }, "the decode let go")
        expect(log.length === 2 && log[1].status === Image.Ready, "let go once decoded: " + JSON.stringify(log))
        var shown = gallery({ "source": ViewerImage.source(folder + "a.jpg") })
        expect(shown.photo.status === Image.Ready, "viewer's photo ready at once, status " + shown.photo.status)
        var cold = gallery({ "source": ViewerImage.source(folder + "b.jpg") })
        expect(cold.photo.status === Image.Loading, "photo not fetched first decodes, status " + cold.photo.status)
        waitFor(function () { return cold.photo.status === Image.Ready }, "the photo not fetched first")
        shown.destroy()
        cold.destroy()
    }

    function checkStopDropsIt() {
        log = []
        prefetch.start(folder + "c.jpg", "image/jpeg")
        prefetch.stop()
        expect(log.length === 2 && log[1].source === "" && log[1].status !== Image.Ready,
               "stop drops a running decode: " + JSON.stringify(log))
    }

    function checkPlaceholders() {
        var cam = gallery({ "placeholder": ViewerImage.placeholder(App, folder + "cam.jpg") })
        var plain = gallery({ "placeholder": ViewerImage.placeholder(App, folder + "a.jpg") })
        var camThumb = named(cam, "placeholderImage")
        var plainThumb = named(plain, "placeholderImage")
        waitFor(function () { return camThumb.status === Image.Ready }, "the camera's thumbnail")
        expect(camThumb.implicitWidth === 80 && camThumb.implicitHeight === 60,
               "placeholder is the camera's thumbnail: " + camThumb.implicitWidth + "x" + camThumb.implicitHeight)
        waitFor(function () { return plainThumb.status === Image.Error }, "no thumbnail in a plain photo")
        cam.destroy()
        plain.destroy()
    }

    Component.onCompleted: {
        pictures.createFolder("ViewerPrefetch")
        waitFor(function () { return pictures.indexOf(folder.slice(0, -1)) >= 0 }, "the folder")
        if (!expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "a.jpg")
                    && tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "b.jpg")
                    && tools.installFixture(Qt.resolvedUrl("host/fixtures/large-photo.jpg"), folder + "c.jpg")
                    && tools.installFixture(Qt.resolvedUrl("host/fixtures/camera.jpg"), folder + "cam.jpg")
                    && tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), folder + "notes.md"),
                    "fixtures installed"))
            return
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        checkOtherFilesNotFetched()
        checkPrefetchFeedsViewer()
        checkStopDropsIt()
        checkPlaceholders()
        reachedEnd = true
    }

    Component {
        id: pageComponent
        Page {}
    }
}
