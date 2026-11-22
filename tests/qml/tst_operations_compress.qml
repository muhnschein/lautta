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

    ViewerTools { id: tools }

    DirectoryModel { id: documents; uri: "lautta://user-documents/" }

    Connections {
        target: Operations
        onJobFinished: test.made = ok ? resultUri : "failed"
        onFailed: console.error("FAILED: " + kind + " " + message)
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

    Component.onCompleted: {
        checkTwoItems()
        checkFormats()
        reachedEnd = true
    }
}
