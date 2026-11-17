// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"
import "../../qml/pages"

ApplicationWindow {
    id: test
    property bool reachedEnd

    ViewerTools { id: tools }

    initialPage: Component { BrowsePage { } }

    BrowseHandlers { }

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

    // Accepts a dialog and leaves its closing transition running.
    function closeDialog() {
        var dialog = pageStack.push(plainDialog)
        if (!waitFor(function () { return !pageStack.busy }, "the dialog to open"))
            return false
        dialog.accept()
        return expect(pageStack.busy, "the dialog closes")
    }

    // Server answers while Connect to server's dialog closes.
    function checkFolderWhileDialogCloses() {
        var dest = "lautta://user-documents/"
        if (!closeDialog())
            return
        Bridge.adhocConnected(dest)
        waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.uri === dest
        }, "the server's folder to open, current page: " + pageStack.currentPage)
        expect(pageStack.depth === 2, "folder opened once: depth " + pageStack.depth)
    }

    // A question that came while a dialog closed would leave the connection waiting forever.
    function checkQuestionWhileDialogCloses() {
        if (!closeDialog())
            return
        Bridge.question("q-closing", "identity-unknown", JSON.stringify({ "host": "server" }))
        waitFor(function () {
            return !pageStack.busy && pageStack.currentPage.questionId === "q-closing"
        }, "the server question to open, current page: " + pageStack.currentPage)
        expect(pageStack.depth === 3, "question opened once: depth " + pageStack.depth)
    }

    Component.onCompleted: {
        checkFolderWhileDialogCloses()
        checkQuestionWhileDialogCloses()
        reachedEnd = true
    }
}
