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
    readonly property string source: "lautta://user-documents/NewFolderFlashSrc/"
    readonly property int filler: 60
    property var flashes: []

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents

        uri: "lautta://user-documents/"
    }

    DirectoryModel { id: here }
    DirectoryModel { id: sources }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function waitFor(done, what) {
        // Hang guard only: emulated target under load is slow.
        for (var i = 0; i < 1200 && !done(); ++i)
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

    function inView(item, view, covered) {
        var top = item.mapToItem(view, 0, 0).y
        return top >= 0 && top + item.height <= view.height - (covered || 0)
    }

    function name(i) {
        return "d" + (i < 10 ? "0" : "") + i
    }

    function setUp() {
        documents.createFolder("NewFolderFlash")
        documents.createFolder("NewFolderFlashSrc")
        documents.refresh()
        waitFor(function () {
            return !documents.loading && documents.nameKind("NewFolderFlash") === "folder"
                   && documents.nameKind("NewFolderFlashSrc") === "folder"
        }, "the test folders")
        here.uri = base
        for (var i = 0; i < filler; ++i)
            here.createFolder(name(i))
        waitFor(function () {
            if (!here.loading && here.count < filler)
                here.refresh()
            return !here.loading && here.count === filler
        }, "the filler folders")
    }

    // Names sort last: new items start out of view at the bottom.
    // `firstOnly`: items far apart; only the first in list order must be in view.
    function checkFlash(view, act, uris, what, firstOnly) {
        flashes = []
        view.flickable.positionViewAtBeginning()
        act()
        if (!waitFor(function () { return flashes.length >= uris.length }, what + ": the flash"))
            return
        expect(JSON.stringify(flashes.slice().sort()) === JSON.stringify(uris.slice().sort()),
               what + ": one flash per new item: " + JSON.stringify(flashes))
        var first = Math.min.apply(null, uris.map(view.model.indexOf))
        for (var i = 0; i < uris.length; ++i) {
            // Out of view far below: no row made for it.
            if (firstOnly && view.model.indexOf(uris[i]) !== first)
                continue
            var flash = flashOf(view.flickable, uris[i])
            if (!expect(flash, what + ": " + uris[i] + " has its highlight"))
                continue
            expect(flash.running, what + ": its highlight runs")
            expect(inView(flash, view.flickable, view.coveredHeight), what + ": " + uris[i] + " is scrolled into view")
        }
        var others = findAll(view.flickable, function (x) { return x.running === true && x.owner === view && uris.indexOf(x.uri) < 0 }, [])
        expect(others.length === 0, what + ": no other row lights up: " + others.length)
    }

    // Tile's name and details take ambience colours with its highlight, then return to plain.
    function checkGridCaption(view, uri, name) {
        var flash = flashOf(view.flickable, uri)
        var names = findAll(view.flickable, function (x) { return x.objectName === "gridName" && x.text === name }, [])
        if (!expect(flash && names.length === 1, "grid caption: tile and name of " + name))
            return
        var caption = names[0].parent
        var details = findAll(caption, function (x) { return x.objectName === "gridCaption0" }, [])
        if (!expect(details.length === 1, "grid caption: details line of " + name))
            return
        if (!waitFor(function () { return !flash.running }, "grid caption: highlight over"))
            return
        expect(Qt.colorEqual(names[0].color, Theme.primaryColor), "grid caption: name back to plain: " + names[0].color)
        expect(Qt.colorEqual(details[0].color, Theme.secondaryColor), "grid caption: details back to plain: " + details[0].color)
        // Fully lit, set by hand: the fade's hold is too short to catch on a stalled runner.
        flash.opacity = 1
        expect(Qt.colorEqual(names[0].color, Theme.highlightColor), "grid caption: name in ambience colour: " + names[0].color)
        expect(Qt.colorEqual(details[0].color, Theme.secondaryHighlightColor),
               "grid caption: details in ambience colour: " + details[0].color)
        flash.opacity = 0
    }

    function checkView(view, made, what) {
        checkFlash(view, function () { view.model.createFolder(made) }, [base + made], what)
    }

    function fixture(uri) {
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), uri), "fixture " + uri)
    }

    function finished(act, what) {
        var done = { "ok": false, "uri": "" }
        var on = function (job, ok, resultUri) { done.ok = ok; done.uri = resultUri }
        Operations.jobFinished.connect(on)
        act()
        waitFor(function () { return done.ok }, what)
        Operations.jobFinished.disconnect(on)
        waitFor(function () { return flashes.indexOf(done.uri) >= 0 }, what + ": its flash")
        return done.uri
    }

    function checkLanded(view) {
        fixture(source + "zz-pasted.md")
        fixture(source + "zz-inner.md")
        fixture(source + "zz-pasted-2.md")
        checkFlash(view, function () { Operations.copyTo(JSON.stringify([source + "zz-pasted.md", source + "zz-pasted-2.md"]), base) },
                   [base + "zz-pasted.md", base + "zz-pasted-2.md"], "paste two")
        // Folders list first: a folder named "a…" sits at the top, a file "zz…" at the bottom.
        sources.uri = source
        sources.createFolder("aa-top")
        waitFor(function () {
            if (!sources.loading && sources.nameKind("aa-top") !== "folder")
                sources.refresh()
            return !sources.loading && sources.nameKind("aa-top") === "folder"
        }, "the folder to paste")
        fixture(source + "zz-bottom.md")
        checkFlash(view, function () {
            view.flickable.positionViewAtEnd()
            Operations.copyTo(JSON.stringify([source + "aa-top", source + "zz-bottom.md"]), base)
        }, [base + "aa-top", base + "zz-bottom.md"], "paste far apart", true)
        // Word of it after the folder's own relisting saw the file: still lights up.
        fixture(base + "zz-late.md")
        waitFor(function () { return !view.model.loading && view.model.indexOf(base + "zz-late.md") >= 0 }, "the late file listed")
        checkFlash(view, function () { view.landed([base + "zz-late.md"], true) }, [base + "zz-late.md"], "late word")
        // Clipboard panel covers the list's bottom: new last row scrolls above it.
        fixture(source + "zz-under-panel.md")
        expect(App.copy(JSON.stringify([source + "zz-under-panel.md"])), "copy for the panel")
        waitFor(function () { return view.coveredHeight >= Theme.itemSizeLarge }, "the clipboard panel's room")
        checkFlash(view, function () { Operations.copyTo(JSON.stringify([source + "zz-under-panel.md"]), base) },
                   [base + "zz-under-panel.md"], "paste under the clipboard panel")
        App.clearClipboard()
        checkFlash(view, function () { Operations.compress(JSON.stringify([source + "zz-inner.md"]), base, "zz-packed", "zip", "") },
                   [base + "zz-packed.zip"], "compress")
        // Extract lands beside the compressed file: one item as is, several in a new folder.
        var one = finished(function () {
            Operations.compress(JSON.stringify([source + "zz-inner.md"]), base, "zz-one", "zip", "")
        }, "the compressed file of one item")
        checkFlash(view, function () { Operations.extract(one, "") }, [base + "zz-inner.md"], "extract one item")
        var two = finished(function () {
            Operations.compress(JSON.stringify([source + "zz-inner.md", source + "zz-pasted.md"]), base, "zz-two", "zip", "")
        }, "the compressed file of two items")
        checkFlash(view, function () { Operations.extract(two, "") }, [base + "zz-two"], "extract into a new folder")
        // Scan says saved once its file is written.
        fixture(base + "zz-scan.pdf")
        waitFor(function () { return !view.model.loading && view.model.indexOf(base + "zz-scan.pdf") >= 0 }, "the scan file listed")
        checkFlash(view, function () { view.endScan(Qt.createQmlObject("import QtQuick 2.6; QtObject {}", window), base + "zz-scan.pdf") },
                   [base + "zz-scan.pdf"], "scan saved")
        fixture(base + "a-rename.md")
        waitFor(function () { return view.model.indexOf(base + "a-rename.md") >= 0 }, "the file to rename")
        checkFlash(view, function () {
            view.renamingDir = false
            view.model.rename(base + "a-rename.md", "zz-renamed.md")
        }, [base + "zz-renamed.md"], "rename")
        view.forgetRename(view.renamed)
    }

    function checkPicker() {
        flashes = []
        var picker = pageStack.push(pickerComponent, {}, PageStackAction.Immediate)
        var list = findAll(picker, function (x) { return x.positionViewAtIndex !== undefined && x.model && x.model.foldersOnly !== undefined }, [])[0]
        waitFor(function () { return !list.model.loading && list.model.count === filler + 4 }, "the picker's folders")
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
        checkLanded(view)
        view.model.setViewPrefs(JSON.stringify({ "viewMode": "grid" }))
        waitFor(function () { return view.gridMode }, "the grid")
        checkView(view, "zy", "grid")
        checkGridCaption(view, base + "zy", "zy")
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
                // Phone-like height: test window leaves less than a row above the panels.
                width: parent.width
                height: 1000
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
