// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import QtQuick.Window 2.2 as QtQuick
import QtTest 1.0
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

QtQuick.Window {
    id: window

    property var __silica_applicationwindow_instance: app
    property var pageStack: app.pageStack

    ViewerTools { id: tools }
    TestEvent { id: events }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function findAll(item, pred, out) {
        if (!item)
            return out
        if (pred(item) && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    function waitFor(done) {
        for (var i = 0; i < 100 && !done(); ++i)
            tools.pump(50)
    }

    function listOf(viewer) {
        return findAll(viewer, function(x) { return x.headerItem !== undefined }, [])[0]
    }

    function openText(uri) {
        var viewer = app.pageStack.push(textComponent, { "uri": uri }, PageStackAction.Immediate)
        var view = listOf(viewer)
        waitFor(function() { return view.count > 0 && !app.pageStack.busy })
        view.forceLayout()
        return viewer
    }

    function swipe(page) {
        var y = page.height * 0.7
        events.mousePress(page, 20, y, Qt.LeftButton, Qt.NoModifier, -1)
        for (var x = 30; x <= page.width - 20; x += 10)
            events.mouseMove(page, x, y, 15, Qt.LeftButton)
        events.mouseRelease(page, page.width - 20, y, Qt.LeftButton, Qt.NoModifier, 15)
        waitFor(function() { return !app.pageStack.busy && app.pageStack.currentPage !== page })
    }

    function swipesBack(viewer, below, what) {
        swipe(viewer)
        if (!expect(app.pageStack.currentPage === below, what + ": a swipe goes back, depth " + app.pageStack.depth))
            app.pageStack.pop(below, PageStackAction.Immediate)
    }

    function vertical(view) {
        return view.flickableDirection === Flickable.VerticalFlick && view.contentWidth <= view.width
    }

    function isOneOf(text, id, english) {
        return text === id || text === english
    }

    function pulleyTexts(viewer) {
        return findAll(viewer, function(x) { return x.hasOwnProperty("__silica_menuitem") }, [])
            .map(function(item) { return item.text })
    }

    function wraps(below) {
        var viewer = openText("lautta://user-documents/wide.txt")
        var view = listOf(viewer)
        expect(viewer.wrap === undefined, "there is no way to unwrap")
        var entries = pulleyTexts(viewer)
        expect(entries.length === 2 && isOneOf(entries[0], "lautta-viewers-open-with", "Open with")
               && isOneOf(entries[1], "lautta-viewers-share", "Share"),
               "the pulley has Open with and Share only: " + JSON.stringify(entries))
        expect(vertical(view), "the list flicks only up and down: direction " + view.flickableDirection
               + ", " + view.contentWidth + " wide in " + view.width)
        expect(Math.abs(view.contentY - view.originY) < 0.5 && view.headerItem.description.trim() !== "",
               "the whole header shows: contentY " + view.contentY + ", originY " + view.originY)
        view.positionViewAtIndex(3, ListView.Beginning)
        view.forceLayout()
        var wide = findAll(view.contentItem, function(x) { return x.text !== undefined && String(x.text).indexOf("let address") === 0 }, [])
        expect(wide.length === 1 && wide[0].lineCount > 1 && wide[0].width < view.width,
               "the wide line wraps: " + (wide.length ? wide[0].lineCount + " rows, " + wide[0].width + " wide" : "not shown"))
        swipesBack(viewer, below, "wrapped")
        var narrow = openText("lautta://user-documents/short.txt")
        expect(vertical(listOf(narrow)), "narrow lines flick only up and down")
        swipesBack(narrow, below, "narrow lines")
    }

    function viewSource() {
        var markdown = app.pageStack.push(markdownComponent, {}, PageStackAction.Immediate)
        tools.pump(300)
        var item = findAll(markdown, function(x) {
            return x.text === "lautta-viewers-view-source" || x.text === "View source"
        }, [])
        if (!expect(item.length === 1, "a View source item"))
            return
        item[0].clicked(null)
        waitFor(function() { return app.pageStack.currentPage !== markdown && !app.pageStack.busy })
        var source = app.pageStack.currentPage
        var view = listOf(source)
        if (!expect(view && findAll(source, function(x) { return x.objectName === "textBackdrop" }, []).length === 1,
                    "View source opens the text viewer"))
            return
        waitFor(function() { return view.count > 0 })
        expect(vertical(view), "the source flicks only up and down")
        swipesBack(source, markdown, "source")
    }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/wide-code.md"), "lautta://user-documents/wide.txt")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/short-lines.txt"), "lautta://user-documents/short.txt")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), "lautta://user-documents/source.md")
        var below = app.pageStack.push(emptyPage, {}, PageStackAction.Immediate)
        wraps(below)
        viewSource()
    }

    width: 540
    height: 960

    ApplicationWindow {
        id: app
    }

    Component {
        id: emptyPage
        Page { }
    }

    Component {
        id: textComponent
        TextViewer { }
    }

    Component {
        id: markdownComponent
        MarkdownViewer { uri: "lautta://user-documents/source.md" }
    }
}
