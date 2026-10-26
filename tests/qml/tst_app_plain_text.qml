// SPDX-License-Identifier: LGPL-2.1-or-later
// The app's own window on the target (real Silica) shows names and server
// texts as plain text (SEC-8). A folder `<b>Bold` holding `<i>Italic.txt`:
// the folder's header and its list show the tags as typed. A server sign-in
// whose name, instruction and prompt carry tags (one an `<img>` with a URL):
// the dialog shows them as typed too. Fails when the window leaves Silica's
// default (auto-detected rich text) or any label showing such text renders
// it as markup. Not the cover: Silica incubates it asynchronously, and the
// check's engine has no incubation controller to finish it.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Item {
    id: root

    readonly property string folderUri: "lautta://user-documents/%3Cb%3EBold"
    // Styled text would make Qt fetch this URL.
    readonly property string instruction: "<img src=\"http://127.0.0.1:9/pixel.png\"> Enter the code"
    property var win

    ViewerTools { id: tools }
    DirectoryModel { id: documents; uri: "lautta://user-documents/" }

    function expect(ok, what) {
        if (!ok)
            console.error(what)
    }

    // Pumps events until `done()` holds, ten seconds at most.
    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        expect(done(), "timed out waiting for " + what)
    }

    // A flickable's content item is one of its children too: each item once.
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

    // Text items whose text carries one of the tags put in the names.
    // Headless, no render loop polishes the views: they are laid out here.
    function tagged(item) {
        var views = findAll(item, function (x) { return typeof x.forceLayout === "function" }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
        return findAll(item, function (x) {
            return x.textFormat !== undefined && typeof x.text === "string" && /<(b|i|u|img)\b/.test(x.text)
        }, [])
    }

    // Whether one of `items` shows exactly `text`.
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

    // Header title and list name.
    function checkFolder() {
        win.pageStack.push(Qt.resolvedUrl("../../qml/pages/DirectoryPage.qml"), { "uri": folderUri + "/" },
                           PageStackAction.Immediate)
        var page = win.pageStack.currentPage
        waitFor(function () { var t = tagged(page); return shows(t, "<b>Bold") && shows(t, "<i>Italic.txt") },
                "the folder's name and its file's name")
        expectPlain(tagged(page), "folder page")
    }

    // Title, instruction, and the prompt as label and placeholder: all of it
    // comes from the server (NVB-6).
    function checkSignIn() {
        win.pageStack.push(Qt.resolvedUrl("../../qml/dialogs/ServerQuestionDialog.qml"), {
            "questionId": "q-markup",
            "kind": "keyboard-interactive",
            "details": {
                "name": "<b>Bank",
                "instruction": instruction,
                "prompts": "<u>Code: "
            }
        }, PageStackAction.Immediate)
        var dialog = win.pageStack.currentPage
        waitFor(function () {
            var t = tagged(dialog)
            return shows(t, "<b>Bank") && shows(t, instruction) && shows(t, "<u>Code")
        }, "the server's texts")
        expectPlain(tagged(dialog), "sign-in dialog")
    }

    Component.onCompleted: {
        makeFolder()
        openWindow()
        if (!win)
            return
        checkFolder()
        checkSignIn()
    }
}
