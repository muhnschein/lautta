// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0
import "../../../qml/components/scan/ScanMath.js" as ScanMath

Item {
    id: root

    property bool reachedEnd
    property var scan: null
    property var ready: []
    property var failures: []
    property var savedUris: []
    readonly property string sheet: Qt.resolvedUrl("fixtures/sheet.jpg")
    readonly property string plain: Qt.resolvedUrl("fixtures/photo.jpg")

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    // Hang guard: work of milliseconds gets half a minute.
    function until(cond, what) {
        for (var i = 0; i < 600 && !cond(); ++i)
            tools.pump(50)
        check(cond(), "waited for: " + what)
    }

    function idle(what) {
        until(function () { return !scan.busy }, what)
    }

    function row(index) {
        return rows.itemAt(index)
    }

    function cornersAre(index, expected) {
        return JSON.stringify(JSON.parse(row(index).corners)) === JSON.stringify(expected)
    }

    function info(index) {
        return JSON.parse(scan.pageJson(index))
    }

    function shows(source) {
        var image = Qt.createQmlObject("import QtQuick 2.6; Image { cache: false }", root)
        image.source = source
        until(function () { return image.status !== Image.Loading }, "picture " + source)
        var result = { "ok": image.status === Image.Ready, "width": image.implicitWidth, "height": image.implicitHeight }
        image.destroy()
        return result
    }

    ViewerTools { id: tools }
    PdfDocument { id: doc }

    Component {
        id: maker

        ScanModel {
            onPhotoReady: root.ready = root.ready.concat([index])
            onFailed: root.failures = root.failures.concat([kind])
            onSaved: root.savedUris = root.savedUris.concat([uri])
        }
    }

    Repeater {
        id: rows

        model: root.scan

        Item {
            readonly property int pageId: model.pageId
            readonly property string view: model.view
            readonly property string picture: model.picture
            readonly property string corners: model.corners
            readonly property int turns: model.turns
            readonly property string colour: model.colour
            readonly property bool edgesFound: model.edgesFound
        }
    }

    function checkPhotos() {
        check(scan.count === 0 && !scan.busy && !scan.saving, "empty at first")
        check(scan.pageJson(0) === "null" && scan.pageJson(-1) === "null", "no page yet")
        var first = scan.shotPath()
        var second = scan.shotPath()
        check(/\/scan\/scan-[^\/]+\/shot-1\.jpg$/.test(first), "first shot in the scan's own folder: " + first)
        check(second === first.replace("shot-1", "shot-2"), "second shot beside it: " + second)

        scan.add(sheet)
        check(scan.busy && scan.count === 0, "working on the photo")
        idle("first page")
        check(scan.count === 1 && rows.count === 1, "one page: " + scan.count)
        check(JSON.stringify(ready) === "[0]", "first page announced: " + JSON.stringify(ready))
        var page = info(0)
        check(page.id === 1 && row(0).pageId === 1, "first page is 1")
        check(page.found && row(0).edgesFound, "sheet's edges found")
        check(ScanMath.isPage(page.quad) && JSON.stringify(page.quad) !== JSON.stringify(ScanMath.whole),
              "corners of the sheet: " + JSON.stringify(page.quad))
        check(cornersAre(0, page.quad), "corners as text: " + row(0).corners)
        check(page.colour === "original" && row(0).colour === "original" && row(0).turns === 0, "plain at first")
        var work = shows("file://" + row(0).picture)
        check(work.ok && work.width === 480 && work.height === 360, "photo to adjust: " + JSON.stringify(work))
        var view = shows("file://" + row(0).view)
        check(view.ok && view.width < 480 && view.height < 360, "page cut out: " + JSON.stringify(view))

        scan.add(plain)
        idle("second page")
        check(scan.count === 2 && JSON.stringify(ready) === "[0,1]", "second page announced: " + JSON.stringify(ready))
        check(!row(1).edgesFound && cornersAre(1, ScanMath.whole), "no edges: whole picture")
        check(row(1).pageId === 2, "second page is 2")
    }

    function checkChanges() {
        var before = row(0).view
        scan.setColour(0, "blackwhite")
        idle("black and white")
        check(row(0).colour === "blackwhite" && row(1).colour === "original", "colour of the named page only")
        check(row(0).view !== before && /\.png$/.test(row(0).view) && shows("file://" + row(0).view).ok,
              "new picture: " + row(0).view)
        scan.setColour(0, "sepia")
        check(JSON.stringify(failures) === '["InvalidArgument"]' && !scan.busy, "unknown colour: " + JSON.stringify(failures))
        check(row(0).colour === "blackwhite", "colour kept")
        scan.setColour(0, "grayscale")
        idle("grayscale")
        check(row(0).colour === "grayscale" && /\.jpg$/.test(row(0).view), "gray: " + row(0).view)

        var upright = shows("file://" + row(1).view)
        scan.turn(1, 1)
        idle("turn")
        var turned = shows("file://" + row(1).view)
        check(row(1).turns === 1 && row(0).turns === 0, "turned once")
        check(turned.width === upright.height && turned.height === upright.width,
              "sides swapped: " + JSON.stringify(upright) + " -> " + JSON.stringify(turned))
        scan.turn(1, -1)
        idle("turn back")
        check(row(1).turns === 0, "turned back: " + row(1).turns)

        var half = [0, 0, 0.5, 0, 0.5, 1, 0, 1]
        scan.setCorners(1, JSON.stringify(half))
        idle("corners")
        check(cornersAre(1, half), "corners set: " + row(1).corners)
        var cut = shows("file://" + row(1).view)
        check(Math.abs(cut.width * 2 - upright.width) <= 2 && cut.height === upright.height, "left half: " + JSON.stringify(cut))
        failures = []
        scan.setCorners(1, "[1, 2]")
        scan.setCorners(1, "nonsense")
        scan.setCorners(1, JSON.stringify([0, 0, 0, 1, 1, 1, 1, 0]))
        check(JSON.stringify(failures) === '["InvalidArgument","InvalidArgument","InvalidArgument"]' && !scan.busy,
              "bad corners: " + JSON.stringify(failures))
        check(cornersAre(1, half), "corners kept")
    }

    function checkWholePicture() {
        scan.setCorners(0, JSON.stringify(ScanMath.whole))
        idle("whole picture")
        check(!row(0).edgesFound && cornersAre(0, ScanMath.whole), "whole picture in use")
    }

    function checkRetakeAndOrder() {
        ready = []
        scan.retake(1, sheet)
        idle("retake")
        check(JSON.stringify(ready) === "[1]", "retaken page announced: " + JSON.stringify(ready))
        check(scan.count === 2 && row(1).pageId === 2 && row(1).edgesFound, "same page, new photo")

        scan.movePage(0, 1)
        check(row(0).pageId === 2 && row(1).pageId === 1, "moved at once: " + row(0).pageId + "," + row(1).pageId)
        scan.movePage(1, 0)
        check(row(0).pageId === 1 && row(1).pageId === 2, "moved back up")
        scan.movePage(0, 1)
        idle("moves")
        check(info(0).id === 2 && info(1).id === 1, "order kept once the work is done")

        failures = []
        scan.turn(9, 1)
        scan.remove(9)
        scan.remove(-1)
        scan.movePage(0, 9)
        scan.movePage(0, 0)
        scan.retake(9, sheet)
        scan.setColour(9, "grayscale")
        check(!scan.busy && failures.length === 0 && scan.count === 2, "pages that are not there are left alone")

        scan.remove(0)
        check(scan.count === 1 && rows.count === 1 && row(0).pageId === 1, "removed at once")
        idle("remove")
        check(scan.count === 1 && info(0).id === 1, "stays removed")
    }

    function checkSaving() {
        failures = []
        scan.save("nope", "x")
        check(JSON.stringify(failures) === '["InvalidArgument"]' && !scan.saving && !scan.busy, "bad folder address")
        scan.save("lautta://user-documents/Missing", "x")
        check(scan.saving && scan.busy, "saving")
        idle("failed save")
        check(JSON.stringify(failures) === '["InvalidArgument","NotFound"]' && !scan.saving,
              "folder that is not there: " + JSON.stringify(failures))
        check(savedUris.length === 0 && scan.count === 1, "pages kept for another try")

        scan.save("lautta://user-documents/", "My scan")
        scan.save("lautta://user-documents/", "Twice")
        idle("save")
        check(!scan.saving && scan.progress > 0.99, "done: " + scan.progress)
        check(JSON.stringify(savedUris) === '["lautta://user-documents/My%20scan.pdf"]', "saved once: " + JSON.stringify(savedUris))
        doc.uri = savedUris[0]
        until(function () { return !doc.loading }, "saved PDF")
        check(doc.loaded && doc.pageCount === 1, "one page in the PDF: " + doc.pageCount)
    }

    function checkLeaving() {
        var photo = "file://" + row(0).picture
        check(shows(photo).ok, "photo there while scanning")
        var old = scan
        scan = null
        old.destroy()
        var gone = false
        until(function () {
            gone = !shows(photo).ok
            return gone
        }, "photos removed with the scan")
    }

    Component.onCompleted: {
        scan = maker.createObject(root)
        checkPhotos()
        checkChanges()
        checkWholePicture()
        checkRetakeAndOrder()
        checkSaving()
        checkLeaving()
        reachedEnd = true
    }
}
