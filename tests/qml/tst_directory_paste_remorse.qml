// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): pasting cut items moves them behind a
// remorse (SPEC OPS-9), from the folder pulley and from the paste bar alike.
// The remorse at the top of the page says what moves; tapping it cancels, so
// nothing moves and the items stay cut; letting it run out moves them and
// takes them off the clipboard; pasting again runs the pending move at once.
// A copy has no remorse. Fails when a cut moves without the remorse, a
// cancelled one moves, a remorse that ran out moves nothing, a pending move
// is lost, or a copy waits.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: window

    readonly property string base: "lautta://user-documents/PasteRemorse/"
    readonly property string from: base + "from/"
    readonly property string to: base + "to/"
    property var remorseBefore

    ViewerTools { id: tools }

    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: top }
    DirectoryModel { id: source }
    DirectoryModel { id: target }

    Component {
        id: pageComponent

        DirectoryPage {
            uri: window.to
        }
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    // Pumps events until `done()` holds, ten seconds at most.
    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + (typeof what === "function" ? what() : what))
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

    // What `model` lists, to say in a failure.
    function names(model) {
        var out = []
        for (var i = 0; i < model.count; ++i)
            out.push(App.nameOf(model.uriAt(i)))
        return JSON.stringify(out)
    }

    // The listing of `model` once it has `name` or has not (`present`). A
    // local folder is watched; asking again, now and then, is for a missed
    // event (asking at every look would keep the model loading).
    function listed(model, name, present) {
        var looks = 0
        model.refresh()
        return waitFor(function () {
            if (++looks % 40 === 0)
                model.refresh()
            return !model.loading && (model.nameKind(name) === "file") === present
        }, function () {
            return name + (present ? " listed in " : " gone from ") + model.uri + ", it lists " + names(model)
        })
    }

    // A text is the id of a string, or its English text where one is loaded.
    function isOneOf(text, ids) {
        return ids.indexOf(text) >= 0
    }

    // The remorse of the folder view, once it has been used.
    function popup() {
        var found = findAll(window.contentItem, function (x) {
            return x.active !== undefined && x._countdown !== undefined && x.execute !== undefined
        }, [])
        return found.length === 1 ? found[0] : null
    }

    function expectRemorse(ids, what) {
        var remorse = popup()
        if (!expect(remorse && remorse.active, what + ": the remorse shows"))
            return null
        expect(isOneOf(remorse.text, ids), what + ": the remorse says " + JSON.stringify(ids) + ", not " + remorse.text)
        return remorse
    }

    // Taps the folder pulley's Paste entry.
    function pulleyPaste() {
        var menus = findAll(window.contentItem, function (x) {
            return x._isPullDownMenu === true && x.flickable && x.flickable.visible
        }, [])
        if (!expect(menus.length === 1, "one folder pulley: " + menus.length))
            return
        menus[0].active = true
        var entries = findAll(menus[0]._contentColumn, function (x) {
            return x.hasOwnProperty("__silica_menuitem") && x.visible
                   && (x.text.indexOf("lautta-dir-menu-paste") === 0 || x.text.indexOf("Paste") === 0)
        }, [])
        if (expect(entries.length === 1, "the pulley has one Paste entry: " + entries.length))
            entries[0].clicked()
        menus[0].active = false
    }

    // Taps Paste here on the paste bar, once it is open.
    function panelPaste() {
        var panels = findAll(window.contentItem, function (x) { return x.fromText !== undefined && x.open !== undefined }, [])
        if (!expect(panels.length === 1, "one paste bar: " + panels.length))
            return
        waitFor(function () { return panels[0].open && panels[0].visibleSize > 0 }, "the paste bar open")
        var buttons = findAll(panels[0], function (x) {
            return x.accent !== undefined && x.description !== undefined
                   && (x.description === "lautta-dir-clip-paste" || x.description === "Paste here")
        }, [])
        if (expect(buttons.length === 1, "one Paste here on the paste bar: " + buttons.length))
            buttons[0].clicked(null)
    }

    function cut(names) {
        expect(App.cut(JSON.stringify(names.map(function (n) { return window.from + n }))), "cut " + names)
    }

    function setUp() {
        documents.createFolder("PasteRemorse")
        waitFor(function () { return documents.nameKind("PasteRemorse") === "folder" }, "the folder")
        top.uri = base
        top.createFolder("from")
        waitFor(function () { return top.nameKind("from") === "folder" }, "the source folder")
        top.createFolder("to")
        waitFor(function () { return top.nameKind("to") === "folder" }, "the target folder")
        var names = ["a.md", "b.md", "c.md", "d.md", "e.md"]
        for (var i = 0; i < names.length; ++i)
            expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), from + names[i]), "fixture " + names[i])
        source.uri = from
        target.uri = to
        waitFor(function () { return !source.loading && source.count === 5 && !target.loading }, "the folders")
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        waitFor(function () {
            var views = findAll(window.contentItem, function (x) { return x.paste !== undefined && x.runAction !== undefined }, [])
            return views.length === 1 && !views[0].model.loading && views[0].model.writable
        }, "the target folder open")
    }

    // From the pulley: the remorse shows, tapping it cancels, nothing moves.
    function checkCancel() {
        App.setSetting("remorse_seconds", JSON.stringify(60))
        cut(["a.md"])
        pulleyPaste()
        var remorse = expectRemorse(["lautta-dir-moving", "Moving “a.md”"], "pulley")
        if (remorse)
            remorse.cancel()
        expect(App.clipboardCut && App.clipboardCount === 1, "a cancelled move leaves the item cut: "
               + App.clipboardCount)
    }

    // From the paste bar: the remorse runs out and the items move. The
    // cancelled move of a.md was started first, so it is over when these two
    // have moved: it must not have moved.
    function checkRunsOut() {
        App.setSetting("remorse_seconds", JSON.stringify(1))
        cut(["b.md", "c.md"])
        panelPaste()
        expectRemorse(["lautta-dir-moving-many", "Moving 2 items"], "paste bar")
        listed(target, "c.md", true)
        listed(target, "b.md", true)
        listed(source, "c.md", false)
        listed(source, "b.md", false)
        expect(source.nameKind("a.md") === "file", "the cancelled move left a.md in place")
        expect(target.nameKind("a.md") === "", "and did not put it in the target")
        expect(App.clipboardCount === 0, "a move that ran takes its items off the clipboard: " + App.clipboardCount)
    }

    function checkCopy() {
        expect(App.copy(JSON.stringify([from + "a.md"])), "copy a.md")
        pulleyPaste()
        var remorse = popup()
        expect(!remorse || !remorse.active, "a copy has no remorse")
        listed(target, "a.md", true)
        expect(source.nameKind("a.md") === "file", "a copy leaves the original")
        expect(App.clipboardCount === 1 && !App.clipboardCut, "and stays on the clipboard")
        App.clearClipboard()
    }

    // Pasting again runs the pending move at once, as Silica does when the
    // page goes: its items move before their timer, and the new move waits.
    function checkPending() {
        App.setSetting("remorse_seconds", JSON.stringify(60))
        cut(["d.md"])
        pulleyPaste()
        expectRemorse(["lautta-dir-moving", "Moving “d.md”"], "first")
        cut(["e.md"])
        pulleyPaste()
        listed(target, "d.md", true)
        var remorse = expectRemorse(["lautta-dir-moving", "Moving “e.md”"], "second")
        if (remorse)
            remorse.cancel()
        listed(source, "d.md", false)
        expect(source.nameKind("e.md") === "file", "the move that waited was cancelled")
        expect(target.nameKind("e.md") === "", "and did not happen")
        expect(App.clipboardCut && App.clipboardCount === 1, "its item is still cut")
        App.clearClipboard()
    }

    Component.onCompleted: {
        remorseBefore = App.setting("remorse_seconds")
        setUp()
        checkCancel()
        checkRunsOut()
        checkCopy()
        checkPending()
        App.setSetting("remorse_seconds", JSON.stringify(remorseBefore))
    }
}
