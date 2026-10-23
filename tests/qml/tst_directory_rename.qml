// SPDX-License-Identifier: LGPL-2.1-or-later
// RenameDialog on the target (real Silica): the plain dialog header titled
// Rename with Silica's own accept and cancel texts, and the name field,
// nothing else. The name is selected without its extension, and a name that
// is taken shows as an error in the field's label; the old bin's name at
// the location's root is free (OPS-8).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/dialogs"

ApplicationWindow {
    id: win

    ViewerTools { id: tools }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
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

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), "lautta://user-documents/taken.md")
        pageStack.push(dialogComponent, {}, PageStackAction.Immediate)
        tools.pump(800)
        var dialog = pageStack.currentPage
        var header = findAll(dialog, function(x) { return x.defaultAcceptText !== undefined }, [])
        var field = findAll(dialog, function(x) { return x.errorHighlight !== undefined && x.visible }, [])
        expect(header.length === 1 && field.length === 1, "one header and one field")
        if (header.length !== 1 || field.length !== 1)
            return
        var column = field[0].parent
        var shown = findAll(column, function(x) { return x !== column && x.parent === column && x.visible && x.height > 0 }, [])
        expect(shown.length === 2 && shown[0] === header[0] && shown[1] === field[0],
               "only the header and the name field: " + shown.length + " items")
        var title = header[0].title
        expect(title === "Rename" || title === "lautta-dir-rename-title", "titled Rename: " + title)
        expect(header[0].acceptText === header[0].defaultAcceptText, "Silica's accept text: " + header[0].acceptText)
        expect(header[0].cancelText === header[0].defaultCancelText, "Silica's cancel text: " + header[0].cancelText)
        expect(field[0].selectedText === "draft-v7", "selected without the extension: " + field[0].selectedText)
        expect(!field[0].errorHighlight && dialog.canAccept === false, "the old name is no error, nor a rename")
        field[0].text = "taken.md"
        expect(field[0].errorHighlight && field[0].label !== field[0].placeholderText && !dialog.canAccept,
               "a taken name is an error in the label: " + field[0].label)
        field[0].text = ".lautta-trash"
        expect(!field[0].errorHighlight && dialog.canAccept, "the old bin's name is free at the root (OPS-8)")
        field[0].text = "fresh.odt"
        expect(!field[0].errorHighlight && dialog.canAccept && dialog.newName === "fresh.odt", "a free name")
    }

    Component {
        id: dialogComponent
        RenameDialog {
            uri: "lautta://user-documents/draft-v7.odt"
        }
    }
}
