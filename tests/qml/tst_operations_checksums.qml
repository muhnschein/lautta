// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/directory/ContextActions.js" as Actions

ApplicationWindow {
    id: window
    property bool reachedEnd

    readonly property string base: "lautta://user-documents/Sums/"
    readonly property string photoSha256: "eaf602095e76dac2dfdd976674990dbf2dede5e706d35f0aee2a9368b6407158"
    property var folderPage

    ViewerTools { id: tools }

    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: here }

    Component {
        id: pageComponent

        DirectoryPage { }
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + (typeof what === "function" ? what() : what))
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

    function named(item, name) {
        return findAll(item, function (x) { return x.objectName === name }, [])
    }

    function folderView() {
        var views = findAll(pageStack.currentPage, function (x) { return x.runSelectionAction !== undefined }, [])
        return views.length === 1 ? views[0] : null
    }

    function layOut() {
        var views = findAll(window.contentItem, function (x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
    }

    function settled(pred, what) {
        var page
        waitFor(function () {
            page = pageStack.currentPage
            return !pageStack.busy && page && pred(page)
        }, what)
        return page
    }

    function moreIds(view) {
        waitFor(function () { return view.moreActionsPage && pageStack.currentPage === view.moreActionsPage && !pageStack.busy },
                "the More actions page")
        return view.moreActionsPage.actions.map(function (a) { return a.id })
    }

    function tapMore(view, id) {
        var list = findAll(view.moreActionsPage, function (x) { return x.positionViewAtIndex !== undefined }, [])[0]
        list.positionViewAtIndex(view.moreActionsPage.actions.map(function (a) { return a.id }).indexOf(id), ListView.Contain)
        layOut()
        var rows = findAll(view.moreActionsPage, function (x) {
            return x.objectName === "moreRow" && findAll(x, function (y) {
                return y.truncationMode !== undefined && y.text === Actions.text(id)
            }, []).length > 0
        }, [])
        if (expect(rows.length === 1, "one row for " + id + ": " + rows.length))
            rows[0].clicked(null)
    }

    function isPicker(p) {
        return p.rows !== undefined && p.uri !== undefined && p.algorithm === undefined
    }

    function sectionShown(page, index, text) {
        var list = findAll(page, function (x) { return x.positionViewAtIndex !== undefined }, [])[0]
        list.positionViewAtIndex(index, ListView.Contain)
        layOut()
        function rowY(i) {
            var label = page.rows[i].label
            var rows = named(list, "algorithmRow").filter(function (r) {
                return findAll(r, function (y) { return y.truncationMode !== undefined && y.text === label }, []).length > 0
            })
            return rows.length === 1 ? rows[0].y : NaN
        }
        var top = index > 0 ? rowY(index - 1) : -Infinity
        var bottom = rowY(index)
        return named(list, "checksumSection").filter(function (x) {
            return x.visible && x.text === text && x.y > top && x.y < bottom
        }).length === 1
    }

    function tapAlgorithm(page, id) {
        var label = page.rows.filter(function (r) { return r.id === id })[0].label
        var list = findAll(page, function (x) { return x.positionViewAtIndex !== undefined }, [])[0]
        list.positionViewAtIndex(page.rows.map(function (r) { return r.id }).indexOf(id), ListView.Contain)
        layOut()
        var rows = named(page, "algorithmRow").filter(function (r) {
            return findAll(r, function (y) { return y.truncationMode !== undefined && y.text === label }, []).length > 0
        })
        if (expect(rows.length === 1, "one row for " + id + ": " + rows.length))
            rows[0].clicked(null)
    }

    function setUp() {
        documents.createFolder("Sums")
        var looks = 0
        waitFor(function () {
            if (++looks % 40 === 0)
                documents.refresh()
            return !documents.loading && documents.nameKind("Sums") === "folder"
        }, "the folder")
        var files = [["photo.jpg", "photo.jpg"], ["empty.txt", "empty.txt"], ["SHA256SUMS", "SHA256SUMS"]]
        for (var i = 0; i < files.length; ++i)
            expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/" + files[i][0]), base + files[i][1]), "fixture " + files[i][1])
        here.uri = base
        waitFor(function () { return !here.loading && here.count === 3 }, "the files")
        folderPage = pageStack.push(pageComponent, { "uri": base }, PageStackAction.Immediate)
        waitFor(function () {
            var view = folderView()
            return view && !view.model.loading && view.model.count === 3
        }, "the folder open")
    }

    function checkItem() {
        var view = folderView()
        view.moreActions({ "uri": base + "photo.jpg", "isDir": false, "mimeType": "image/jpeg" })
        var ids = moreIds(view)
        expect(ids.indexOf("checksums") >= 0 && ids.indexOf("verify") < 0, "a photo: Checksums, no Verify files: " + JSON.stringify(ids))
        tapMore(view, "checksums")
        var picker = settled(isPicker, "the Checksums page")
        expect(pageStack.depth === 2, "Checksums takes More's place: depth " + pageStack.depth)
        expect(picker.uri === base + "photo.jpg", "for the photo: " + picker.uri)
        expect(JSON.stringify(picker.rows.map(function (r) { return r.id + ":" + r.section })) === JSON.stringify(
                   ["sha256:common", "md5:common", "sha1:common", "sha512:common", "sha224:more", "sha384:more",
                    "sha3-256:more", "sha3-512:more", "blake2b:more", "blake3:more", "crc32:more", "xxh64:more", "xxh128:more"]),
               "four Common, then nine More, on one page")
        expect(sectionShown(picker, 0, qsTrId("lautta-checksum-common"))
               && sectionShown(picker, 4, qsTrId("lautta-checksum-section-more")), "section headers Common and More")
        tapAlgorithm(picker, "sha256")
        var result = settled(function (p) { return p.algorithm !== undefined && p.model !== undefined }, "the SHA-256 result")
        expect(result.algorithm === "sha256", "SHA-256 chosen: " + result.algorithm)
        waitFor(function () { return !result.model.running }, "SHA-256")
        expect(named(result, "checksumHex")[0].text === photoSha256, "the photo's SHA-256: " + result.model.hex)
        expect(named(result, "checksumVerdict")[0].visible && result.model.verdictList === "SHA256SUMS",
               "SHA256SUMS beside it matches: " + result.model.verdict + " " + result.model.verdictList)
        expect(!named(result, "checksumProgress")[0].visible && !named(result, "checksumError")[0].visible, "done, no error")
        Clipboard.text = ""
        named(result, "checksumValue")[0].clicked(null)
        expect(Clipboard.text === photoSha256, "tap copies the value: " + Clipboard.text)

        pageStack.pop(picker, PageStackAction.Immediate)
        tapAlgorithm(picker, "blake3")
        result = settled(function (p) { return p.algorithm === "blake3" }, "the BLAKE3 result")
        waitFor(function () { return !result.model.running }, "BLAKE3")
        expect(result.model.hex.length === 64 && !named(result, "checksumVerdict")[0].visible,
               "BLAKE3, no list names it: no verdict")
        pageStack.pop(folderPage, PageStackAction.Immediate)
    }

    function checkList() {
        var view = folderView()
        view.moreActions({ "uri": base + "SHA256SUMS", "isDir": false, "mimeType": "text/plain" })
        var ids = moreIds(view)
        expect(ids[0] === "verify" && ids.indexOf("checksums") >= 0, "a list: Verify files first: " + JSON.stringify(ids))
        tapMore(view, "verify")
        var page = settled(function (p) { return p.model !== undefined && p.model.checked !== undefined }, "Verify files")
        expect(pageStack.depth === 2, "Verify files takes More's place: depth " + pageStack.depth)
        waitFor(function () { return !page.model.running }, "the files checked")
        var list = findAll(page, function (x) { return x.positionViewAtEnd !== undefined }, [])[0]
        layOut()
        var rows = named(page, "verifyRow")
        list.positionViewAtEnd()
        layOut()
        named(page, "verifyRow").forEach(function (r) { if (rows.indexOf(r) < 0) rows.push(r) })
        expect(rows.length === 3 && page.model.checked === 3, "three rows: " + rows.length + ", " + page.model.checked + " checked")
        rows.sort(function (a, b) { return a.y - b.y })
        var texts = rows.map(function (r) {
            return findAll(r, function (x) { return x.truncationMode !== undefined && x.text.length > 0 }, []).map(function (l) { return l.text }).join("|")
        })
        expect(texts[0].indexOf("empty.txt") >= 0 && texts[1].indexOf("missing.bin") >= 0 && texts[2].indexOf("photo.jpg") >= 0,
               "problems first: " + JSON.stringify(texts))
        pageStack.pop(folderPage, PageStackAction.Immediate)
    }

    function checkSelection() {
        var view = folderView()
        view.startSelection()
        view.model.select(base + "photo.jpg")
        view.moreSelection()
        var ids = moreIds(view)
        expect(ids.indexOf("checksums") >= 0, "one selected file: Checksums: " + JSON.stringify(ids))
        tapMore(view, "checksums")
        var picker = settled(isPicker, "Checksums from the selection")
        expect(picker.uri === base + "photo.jpg", "for the selected photo: " + picker.uri)
        waitFor(function () { return !view.selecting }, "selection ends")
        pageStack.pop(folderPage, PageStackAction.Immediate)
        view.startSelection()
        view.model.select(base + "photo.jpg")
        view.model.select(base + "empty.txt")
        view.moreSelection()
        ids = moreIds(view)
        expect(ids.indexOf("checksums") < 0 && ids.indexOf("verify") < 0, "two files: neither: " + JSON.stringify(ids))
        pageStack.pop(folderPage, PageStackAction.Immediate)
        view.endSelection()
    }

    Component.onCompleted: {
        setUp()
        checkItem()
        checkList()
        checkSelection()
        reachedEnd = true
    }
}
