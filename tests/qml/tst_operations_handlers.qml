// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"

ApplicationWindow {
    id: window
    property bool reachedEnd

    ViewerTools { id: tools }

    initialPage: Component {
        Page { }
    }

    OperationsHandlers {
        id: handlers

        shareEnabled: false
    }

    Component {
        id: plainPage

        Page { }
    }

    Component {
        id: plainDialog

        Dialog { }
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

    function checkSummaryWhileDialogCloses() {
        var depth = pageStack.depth
        var dialog = pageStack.push(plainDialog)
        if (!waitFor(function () { return !pageStack.busy }, "the dialog to open"))
            return
        dialog.accept()
        if (!expect(pageStack.busy, "the dialog closes"))
            return
        Operations.needsSummary(7, JSON.stringify({ "kind": "copy", "files": 1500 }))
        waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.planId === 7
        }, "the plan summary to open, current page: " + pageStack.currentPage)
        expect(pageStack.depth === depth + 1, "summary once: depth " + pageStack.depth)
    }

    function checkSummaryWhenIdle() {
        if (!expect(!pageStack.busy, "the stack is idle"))
            return
        Operations.needsSummary(8, JSON.stringify({ "kind": "copy", "files": 1500 }))
        waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.planId === 8
        }, "the plan summary to open on an idle stack")
    }

    function checkShareWhileDialogCloses() {
        var depth = pageStack.depth
        var dialog = pageStack.push(plainDialog)
        if (!waitFor(function () { return !pageStack.busy }, "the dialog to open"))
            return
        dialog.accept()
        if (!expect(pageStack.busy, "the dialog closes"))
            return
        handlers.receiveShare([ "file:///home/user/Downloads/shared.jpg" ])
        waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.resources !== undefined
        }, "the share target to open, current page: " + pageStack.currentPage)
        expect(pageStack.depth === depth + 1 && pageStack.currentPage.resources.length === 1,
               "share target once: depth " + pageStack.depth)
    }

    function checkLockedWhileDialogCloses() {
        var depth = pageStack.depth
        var dialog = pageStack.push(plainDialog)
        if (!waitFor(function () { return !pageStack.busy }, "the dialog to open"))
            return
        dialog.accept()
        if (!expect(pageStack.busy, "the dialog closes"))
            return
        Operations.locked("lautta://user-documents/missing.zip", "open", true)
        if (!waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.wrong !== undefined
        }, "the Locked file page to open, current page: " + pageStack.currentPage))
            return
        var locked = pageStack.currentPage
        expect(pageStack.depth === depth + 1 && locked.wrong, "asked once, saying the last one was wrong")
        var failed = ""
        var record = function (kind) { failed = kind }
        Operations.failed.connect(record)
        var fields = []
        var seen = []
        var find = function (item) {
            if (!item || seen.indexOf(item) >= 0)
                return
            seen.push(item)
            if (item.echoMode !== undefined && item.label !== undefined)
                fields.push(item)
            var kids = item.children || []
            for (var i = 0; i < kids.length; ++i)
                find(kids[i])
            find(item.contentItem)
        }
        find(locked)
        fields[0].text = "secret"
        locked.accept()
        expect(locked.password === "", "accepted: the page forgets the password")
        waitFor(function () { return failed.length > 0 }, "the open to be tried")
        Operations.failed.disconnect(record)
        expect(failed === "NotFound", "tried with the password: " + failed)
        waitFor(function () { return !pageStack.busy }, "the page to close")
    }

    function checkArchiveWhileDialogCloses() {
        var depth = pageStack.depth
        var dialog = pageStack.push(plainDialog)
        if (!waitFor(function () { return !pageStack.busy }, "the dialog to open"))
            return
        dialog.accept()
        if (!expect(pageStack.busy, "the dialog closes"))
            return
        Operations.archiveOpened("lautta://user-documents/a.zip", "lautta://user-downloads/")
        waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.uri === "lautta://user-downloads/"
        }, "the folder page to open, current page: " + pageStack.currentPage)
        expect(pageStack.depth === depth + 1, "folder page once: depth " + pageStack.depth)
    }

    Component.onCompleted: {
        checkLockedWhileDialogCloses()
        checkArchiveWhileDialogCloses()
        checkSummaryWhileDialogCloses()
        checkSummaryWhenIdle()
        checkShareWhileDialogCloses()
        reachedEnd = true
    }
}
