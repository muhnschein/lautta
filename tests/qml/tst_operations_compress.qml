// SPDX-License-Identifier: LGPL-2.1-or-later
// Headless: no finger, no render loop; row looks unproven here.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/dialogs"

ApplicationWindow {
    id: test
    property bool reachedEnd

    readonly property string base: "lautta://user-documents/CompressPage/"
    property string made
    property var asked: []
    property string opened

    ViewerTools { id: tools }

    DirectoryModel { id: documents; uri: "lautta://user-documents/" }

    Connections {
        target: Operations
        onJobFinished: test.made = ok ? resultUri : "failed"
        onFailed: console.error("FAILED: " + kind + " " + message)
        onLocked: test.asked = test.asked.concat([action + (wrong ? " wrong" : "")])
        onArchiveOpened: test.opened = rootUri
    }

    Component {
        id: dialogComponent

        CompressDialog { }
    }

    initialPage: Component {
        Page { }
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

    function rows(dialog) {
        return findAll(dialog, function (x) { return x.objectName === "compressFormat" }, [])
    }

    function titleColour(row) {
        return findAll(row, function (x) { return x.wrapMode !== undefined && x.text !== undefined }, [])[0].color
    }

    function checkTwoItems() {
        var dialog = pageStack.push(dialogComponent, {
            "uris": [ "lautta://user-documents/a.txt", "lautta://user-documents/b.txt" ]
        }, PageStackAction.Immediate)
        expect(dialog.folderUri === "lautta://user-documents/", "the file goes next to the items: " + dialog.folderUri)
        expect(dialog.defaultName() === App.locationName("lautta://user-documents/"),
               "two items at a place's top are named after the place: " + dialog.defaultName())
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function checkFormats() {
        documents.createFolder("CompressPage")
        waitFor(function () { return documents.indexOf(base.replace(/\/$/, "")) >= 0 }, "the folder")
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "notes.md"), "fixture")
        var dialog = pageStack.push(dialogComponent, { "uris": [ base + "notes.md" ] }, PageStackAction.Immediate)
        var list = rows(dialog)
        expect(JSON.stringify(dialog.formats.map(function (f) { return f.id })) === JSON.stringify(["zip", "7z", "tar.xz", "tar.gz"]),
               "four formats")
        if (!expect(list.length === 4, "one row per format: " + list.length))
            return
        expect(dialog.format === "zip", "Zip by default: " + dialog.format)
        expect(dialog.defaultName() === "notes", "one item names the file: " + dialog.defaultName())
        expect(Qt.colorEqual(titleColour(list[0]), Theme.highlightColor), "the picked row stands out")
        expect(Qt.colorEqual(titleColour(list[2]), Theme.primaryColor), "the others do not")
        list[2].clicked(null)
        expect(dialog.format === "tar.xz", "a tap picks the format: " + dialog.format)
        expect(Qt.colorEqual(titleColour(list[2]), Theme.highlightColor) && Qt.colorEqual(titleColour(list[0]), Theme.primaryColor),
               "the highlight follows the pick")
        expect(dialog.canAccept, "a name and a folder: can accept")
        dialog.accept()
        waitFor(function () { return test.made.length > 0 }, "the compressed file")
        expect(test.made === base + "notes.tar.xz", "made in the picked format beside the item: " + test.made)
    }

    function named(dialog, name) {
        return findAll(dialog, function (x) { return x.objectName === name }, [])[0]
    }

    function labelColour(field) {
        return findAll(field, function (x) { return x.editor === field }, [])[0].color
    }

    function checkPassword() {
        waitFor(function () { return !pageStack.busy }, "the last page to close")
        var dialog = pageStack.push(dialogComponent, { "uris": [ base + "notes.md" ] }, PageStackAction.Immediate)
        var password = named(dialog, "compressPassword")
        var confirm = named(dialog, "compressConfirm")
        expect(password.visible && confirm.visible, "Zip takes a password")
        dialog.format = "tar.gz"
        expect(!password.visible && !confirm.visible && dialog.password === "", "tar takes none")
        dialog.format = "7z"
        expect(password.visible, "7z takes one")

        password.text = "summer2024"
        expect(dialog.weakPassword && password.label === qsTrId("lautta-compress-weak-password"),
               "a guessable password is called weak: " + password.label)
        expect(Qt.colorEqual(labelColour(password), Theme.presenceColor(Theme.PresenceAway)), "in orange")
        expect(!dialog.canAccept && !confirm.errorHighlight, "not confirmed yet: no error, no Compress")
        confirm.text = "summer2023"
        expect(!dialog.canAccept && confirm.errorHighlight && confirm.label === qsTrId("lautta-compress-passwords-differ"),
               "a different confirmation is said: " + confirm.label)
        confirm.text = "summer2024"
        expect(dialog.canAccept && !confirm.errorHighlight && confirm.label === qsTrId("lautta-compress-confirm-password"),
               "matching: Compress, even with a weak password")

        password.text = "correct horse battery staple"
        confirm.text = password.text
        expect(!dialog.weakPassword && password.label === qsTrId("lautta-compress-password"), "a strong one is no warning")
        expect(!Qt.colorEqual(labelColour(password), Theme.presenceColor(Theme.PresenceAway)), "and not orange")
        test.made = ""
        dialog.accept()
        expect(password.text === "" && confirm.text === "", "accepted: the page forgets the password")
        waitFor(function () { return test.made.length > 0 }, "the locked file")
        if (!expect(test.made === base + "notes.7z", "locked 7z made: " + test.made))
            return
        Operations.openArchive(test.made, "")
        waitFor(function () { return test.asked.length === 1 }, "the password question")
        Operations.openArchive(test.made, "correct horse battery staple")
        waitFor(function () { return test.opened.length > 0 }, "the locked file to open")
        expect(JSON.stringify(test.asked) === JSON.stringify(["open"]), "asked once: " + JSON.stringify(test.asked))
    }

    Component.onCompleted: {
        checkTwoItems()
        checkFormats()
        checkPassword()
        reachedEnd = true
    }
}
