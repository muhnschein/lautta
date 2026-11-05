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

    function shows(page, part) {
        return texts(page, []).some(function (t) { return t.indexOf(part) >= 0 })
    }

    Component.onCompleted: {
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), "lautta://user-documents/doc.md"))
            console.error("FAILED: fixture")
        var page = pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return shows(page, "Title") && shows(page, "emphasis") }, "the document's text")
        reachedEnd = true
    }

    Component {
        id: pageComponent
        MarkdownViewer { uri: "lautta://user-documents/doc.md" }
    }
}
