// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Item {
    id: root
    property bool reachedEnd

    readonly property string folderUri: "lautta://user-documents/%3Cb%3EBold"
    property var win

    ViewerTools { id: tools }
    DirectoryModel { id: documents; uri: "lautta://user-documents/" }

    function expect(ok, what) {
        if (!ok)
            console.error(what)
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        expect(done(), "timed out waiting for " + what)
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

    function tagged(item) {
        var views = findAll(item, function (x) { return typeof x.forceLayout === "function" }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
        return findAll(item, function (x) {
            return x.textFormat !== undefined && typeof x.text === "string" && /<(b|i|u|img)\b/.test(x.text)
        }, [])
    }

    function shows(items, text) {
        for (var i = 0; i < items.length; ++i) {
            if (items[i].text === text)
                return true
        }
        return false
    }

    function expectPlain(items, where) {
        for (var i = 0; i < items.length; ++i)
            expect(items[i].textFormat === Text.PlainText,
                   where + ": \"" + items[i].text + "\" shows as markup (format " + items[i].textFormat + ")")
    }

    function makeFolder() {
        documents.createFolder("<b>Bold")
        waitFor(function () { return documents.indexOf(folderUri) >= 0 }, "the folder")
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/notes-crlf.txt"), folderUri + "/%3Ci%3EItalic.txt"),
               "the file in it")
    }

    function openWindow() {
        var component = Qt.createComponent(Qt.resolvedUrl("../../qml/harbour-lautta.qml"))
        expect(component.status === Component.Ready, "the app's window: " + component.errorString())
        win = component.createObject(null)
        waitFor(function () { return win && win.pageStack.currentPage && !win.pageStack.busy }, "Browse")
    }

    function checkFolder() {
        win.pageStack.push(Qt.resolvedUrl("../../qml/pages/DirectoryPage.qml"), { "uri": folderUri + "/" },
                           PageStackAction.Immediate)
        var page = win.pageStack.currentPage
        waitFor(function () { var t = tagged(page); return shows(t, "<b>Bold") && shows(t, "<i>Italic.txt") },
                "the folder's name and its file's name")
        expectPlain(tagged(page), "folder page")
    }

    Component.onCompleted: {
        makeFolder()
        openWindow()
        if (!win)
            return
        checkFolder()
        reachedEnd = true
    }
}
