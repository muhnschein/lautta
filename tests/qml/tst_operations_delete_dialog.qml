// SPDX-License-Identifier: LGPL-2.1-or-later
// DeleteDialog on the target (real Silica): a dialog header asking Delete
// permanently? with Delete to accept, and a text that names the item or the
// count, where it is, and that it can't be undone (OPS-8). Nothing to
// delete can't be accepted.
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

    // Engineering English, or the id where no translation is loaded.
    function either(text, english, id) {
        return text === english || text === id
    }

    Component.onCompleted: {
        pageStack.push(dialogComponent, { "uris": ["lautta://user-documents/Uni/thesis.odt"] }, PageStackAction.Immediate)
        tools.pump(300)
        var dialog = pageStack.currentPage
        var header = findAll(dialog, function(x) { return x.defaultAcceptText !== undefined }, [])
        expect(header.length === 1, "one dialog header")
        if (header.length !== 1)
            return
        expect(either(header[0].title, "Delete permanently?", "lautta-delete-permanent-title"), "the question: " + header[0].title)
        expect(either(header[0].acceptText, "Delete", "lautta-delete-permanent-accept"), "accept is Delete: " + header[0].acceptText)
        expect(header[0].cancelText === header[0].defaultCancelText, "Silica's cancel text")
        expect(dialog.canAccept, "one item can be deleted")
        var one = dialog.message()
        expect(one === "lautta-delete-permanent-one"
               || (one.indexOf("“thesis.odt”") >= 0 && one.indexOf("Documents") >= 0 && one.indexOf("can't be undone") >= 0),
               "names the item, its place and that it can't be undone: " + one)
        var shown = findAll(dialog, function(x) { return x.wrapMode === Text.Wrap && x.text === one && x.visible }, [])
        expect(shown.length === 1, "the text is shown: " + shown.length)

        dialog.uris = ["lautta://user-documents/a", "lautta://user-documents/b", "lautta://user-documents/c"]
        var many = dialog.message()
        expect(many === "lautta-delete-permanent-many" || (many.indexOf("3 items") >= 0 && many.indexOf("Documents") >= 0),
               "counts the items: " + many)
        dialog.uris = []
        expect(!dialog.canAccept, "nothing to delete can't be accepted")
    }

    Component {
        id: dialogComponent
        DeleteDialog { }
    }
}
