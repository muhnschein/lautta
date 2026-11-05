// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"

ApplicationWindow {
    id: window

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

    // An extract that ends while a page still moves, e.g. the extract dialog closing.
    function checkFolderAfterTransition() {
        var dest = "lautta://user-documents/"
        pageStack.push(plainPage)
        if (!expect(pageStack.busy, "the push animates"))
            return
        Operations.extractFinished(1, true, dest)
        waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.uri === dest
        }, "the extracted folder to open, current page: " + pageStack.currentPage)
        expect(pageStack.depth === 3 && handlers.pending.length === 0, "opened once: depth " + pageStack.depth)
    }

    // A large extract's plan summary that arrives while the extract dialog closes.
    function checkSummaryWhileDialogCloses() {
        var dialog = pageStack.push(plainDialog)
        if (!waitFor(function () { return !pageStack.busy }, "the dialog to open"))
            return
        dialog.accept()
        if (!expect(pageStack.busy, "the dialog closes"))
            return
        Operations.needsSummary(7, JSON.stringify({ "kind": "extract", "files": 1500 }))
        waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.planId === 7
        }, "the plan summary to open, current page: " + pageStack.currentPage)
        expect(pageStack.depth === 4 && handlers.pending.length === 0, "summary once: depth " + pageStack.depth)
    }

    function checkSummaryWhenIdle() {
        if (!expect(!pageStack.busy, "the stack is idle"))
            return
        Operations.needsSummary(8, JSON.stringify({ "kind": "copy", "files": 1500 }))
        waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.planId === 8
        }, "the plan summary to open on an idle stack")
    }

    Component.onCompleted: {
        checkFolderAfterTransition()
        checkSummaryWhileDialogCloses()
        checkSummaryWhenIdle()
    }
}
