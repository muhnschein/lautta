// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"

ApplicationWindow {
    id: test
    property bool reachedEnd

    ViewerTools { id: tools }

    initialPage: Component { Page { } }

    TransfersHandlers {
        id: handlers
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

    function conflictJson(name) {
        return JSON.stringify({ "name": name, "kind": "copy", "choices": [ "Skip", "Replace", "KeepBoth" ], "open": 1 })
    }

    function checkConflictWhileDialogCloses() {
        var depth = pageStack.depth
        var dialog = pageStack.push(plainDialog)
        if (!waitFor(function () { return !pageStack.busy }, "the dialog to open"))
            return
        dialog.accept()
        if (!expect(pageStack.busy, "the dialog closes"))
            return
        Transfers.needsAnswer(41, 0, conflictJson("big.bin"))
        if (!waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.transferId === 41
        }, "the conflict screen to open, current page: " + pageStack.currentPage))
            return
        var conflict = pageStack.currentPage
        expect(pageStack.depth === depth + 1 && conflict.name === "big.bin",
               "conflict screen once: depth " + pageStack.depth)
        conflict.reject()
        waitFor(function () { return !pageStack.busy && pageStack.depth === depth }, "the conflict screen to close")
    }

    function checkConflictWhenIdle() {
        var depth = pageStack.depth
        if (!expect(!pageStack.busy, "the stack is idle"))
            return
        Transfers.needsAnswer(42, 3, conflictJson("notes.txt"))
        if (!waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.transferId === 42
        }, "the conflict screen to open on an idle stack"))
            return
        expect(pageStack.depth === depth + 1 && pageStack.currentPage.item === 3,
               "conflict screen once on an idle stack: depth " + pageStack.depth)
        pageStack.currentPage.reject()
        waitFor(function () { return !pageStack.busy && pageStack.depth === depth }, "the conflict screen to close")
    }

    Component.onCompleted: {
        checkConflictWhileDialogCloses()
        checkConflictWhenIdle()
        reachedEnd = true
    }
}
