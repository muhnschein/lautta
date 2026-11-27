// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window
    property bool reachedEnd

    ViewerTools { id: tools }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        if (!done())
            console.error("FAILED: timed out waiting for " + what)
        return done()
    }

    function texts(item, out) {
        if (!item)
            return out
        if (typeof item.text === "string" && item.visible)
            out.push(item.text)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            texts(kids[i], out)
        if (item.contentItem)
            texts(item.contentItem, out)
        return out
    }

    function named(item, name, out) {
        if (!item)
            return out
        if (item.objectName === name)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            named(kids[i], name, out)
        if (item.contentItem)
            named(item.contentItem, name, out)
        return out
    }

    function drawn(page, name) {
        return named(page, name, []).some(function (o) { return o.visible && o.width > 0 && o.height > 0 })
    }

    function shows(page, part) {
        return texts(page, []).some(function (t) { return t.indexOf(part) >= 0 })
    }

    Component.onCompleted: {
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), "lautta://user-documents/doc.md"))
            console.error("FAILED: fixture")
        var page = pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return shows(page, "Title") && shows(page, "emphasis") }, "the document's text")
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/styled.md"), "lautta://user-documents/styled.md"))
            console.error("FAILED: styled fixture")
        var styled = pageStack.push(pageComponent, { "uri": "lautta://user-documents/styled.md" }, PageStackAction.Immediate)
        if (waitFor(function () { return shows(styled, "a quote that") }, "the quote")
                && (!drawn(styled, "quoteBar") || drawn(styled, "codePanel")))
            console.error("FAILED: a quote gets a bar, nothing a code panel")
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/code-lines.md"), "lautta://user-documents/code.md"))
            console.error("FAILED: code fixture")
        var code = pageStack.push(pageComponent, { "uri": "lautta://user-documents/code.md" }, PageStackAction.Immediate)
        if (waitFor(function () { return shows(code, "second") }, "the code") && !drawn(code, "codePanel"))
            console.error("FAILED: code sits on a panel")
        if (!tools.installRepeated(Qt.resolvedUrl("host/fixtures/styled.md"), "lautta://user-documents/big.md", 1000))
            console.error("FAILED: big fixture")
        var big = pageStack.push(pageComponent, { "uri": "lautta://user-documents/big.md" }, PageStackAction.Immediate)
        var view = named(big, "readingView", [])[0]
        if (waitFor(function () { return view.count > 1000 }, "the big document")) {
            var made = named(view, "quoteBar", []).length
            if (made === 0 || made > 100)
                console.error("FAILED: only blocks near the screen are laid out: " + made + " of " + view.count)
        }
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/wide-prose.txt"), "lautta://user-documents/notes.txt"))
            console.error("FAILED: prose fixture")
        var prose = pageStack.push(proseComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return shows(prose, "Lake trip notes") && shows(prose, "&lt;b&gt;not bold") }, "plain text as escaped prose")
        reachedEnd = true
    }

    Component {
        id: pageComponent
        MarkdownViewer { uri: "lautta://user-documents/doc.md" }
    }

    Component {
        id: proseComponent
        MarkdownViewer { uri: "lautta://user-documents/notes.txt" }
    }
}
