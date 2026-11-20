// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"
import "../../qml/dialogs"

ApplicationWindow {
    id: window
    property bool reachedEnd

    readonly property string base: "lautta://user-documents/NewFolderFlash/"
    readonly property int filler: 60
    property var flashes: []

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents

        uri: "lautta://user-documents/"
    }

    DirectoryModel { id: here }

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

    function findAll(item, pred, out, seen) {
        seen = seen || []
        if (!item || seen.indexOf(item) >= 0)
            return out
        seen.push(item)
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out, seen)
        if (item.contentItem)
            findAll(item.contentItem, pred, out, seen)
        return out
    }

    // No render loop headless: lay out by hand.
    function layOut(root) {
        var views = findAll(root, function (x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
    }

    function flashOf(root, uri) {
        layOut(root)
        var found = findAll(root, function (x) { return x.running !== undefined && x.owner !== undefined && x.uri === uri }, [])
        return found.length === 1 ? found[0] : null
    }

    function inView(item, view) {
        var top = item.mapToItem(view, 0, 0).y
        return top >= 0 && top + item.height <= view.height
    }

    function name(i) {
        return "d" + (i < 10 ? "0" : "") + i
    }

    function setUp() {
        documents.createFolder("NewFolderFlash")
        documents.refresh()
        waitFor(function () { return !documents.loading && documents.nameKind("NewFolderFlash") === "folder" }, "the test folder")
        here.uri = base
        for (var i = 0; i < filler; ++i)
            here.createFolder(name(i))
        waitFor(function () {
            if (!here.loading && here.count < filler)
                here.refresh()
            return !here.loading && here.count === filler
        }, "the filler folders")
    }

    // Name sorts last: the new folder starts out of view at the bottom.
    function checkView(view, made, what) {
        flashes = []
        view.flickable.positionViewAtBeginning()
        view.model.createFolder(made)
        var uri = base + made
        if (!waitFor(function () { return flashes.length > 0 }, what + ": the flash"))
            return
        expect(JSON.stringify(flashes) === JSON.stringify([uri]), what + ": one flash, for the new folder: " + JSON.stringify(flashes))
        var flash = flashOf(view.flickable, uri)
        if (!expect(flash, what + ": the new folder has its highlight"))
            return
        expect(flash.running, what + ": its highlight runs")
        expect(inView(flash, view.flickable), what + ": the new folder is scrolled into view")
        var others = findAll(view.flickable, function (x) { return x.running === true && x.owner === view && x !== flash }, [])
        expect(others.length === 0, what + ": no other row lights up: " + others.length)
    }

    function checkPicker() {
        flashes = []
        var picker = pageStack.push(pickerComponent, {}, PageStackAction.Immediate)
        var list = findAll(picker, function (x) { return x.positionViewAtIndex !== undefined && x.model && x.model.foldersOnly !== undefined }, [])[0]
        waitFor(function () { return !list.model.loading && list.model.count === filler + 2 }, "the picker's folders")
        picker.flashed.connect(function (itemUri) { window.flashes.push(itemUri) })
        list.model.createFolder("zz")
        var uri = base + "zz"
        if (!waitFor(function () { return flashes.length > 0 }, "picker: the flash"))
            return
        expect(JSON.stringify(flashes) === JSON.stringify([uri]), "picker: one flash, for the new folder: " + JSON.stringify(flashes))
        var flash = flashOf(list, uri)
        if (expect(flash, "picker: the new folder has its highlight")) {
            expect(flash.running, "picker: its highlight runs")
            expect(inView(flash, list), "picker: the new folder is scrolled into view")
        }
        pageStack.pop(null, PageStackAction.Immediate)
    }

    Component.onCompleted: {
        App.setSetting("view_mode", JSON.stringify("list"))
        setUp()
        var page = pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        var view = page.view
        view.flashed.connect(function (itemUri) { window.flashes.push(itemUri) })
        waitFor(function () { return !view.model.loading && view.model.count === filler }, "the folder")
        checkView(view, "zx", "list")
        view.model.setViewPrefs(JSON.stringify({ "viewMode": "grid" }))
        waitFor(function () { return view.gridMode }, "the grid")
        checkView(view, "zy", "grid")
        pageStack.pop(null, PageStackAction.Immediate)
        checkPicker()
        reachedEnd = true
    }

    Component {
        id: pageComponent

        Page {
            property alias view: folder

            DirectoryView {
                id: folder
                anchors.fill: parent
                uri: window.base
            }
        }
    }

    Component {
        id: pickerComponent

        FolderPickerDialog {
            title: "Save to"
            acceptText: "Save"
            startUri: window.base
        }
    }
}
