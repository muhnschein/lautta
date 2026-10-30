// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function settle(o) {
        for (var i = 0; i < 1200 && o.loading; ++i)
            tools.pump(25)
        tools.pump(25)
    }

    ViewerTools { id: tools }
    MarkdownDocument { id: md }

    Text {
        id: preview

        width: 320
        textFormat: Text.RichText
        wrapMode: Text.Wrap
        font.pixelSize: 24
    }

    function laidOut(fixture) {
        var uri = "lautta://user-documents/" + fixture
        check(tools.installFixture(Qt.resolvedUrl("fixtures/" + fixture), uri), fixture + " installed")
        md.uri = uri
        settle(md)
        check(md.html.length > 0, fixture + " rendered")
        preview.text = md.html
        return preview.contentWidth
    }

    function fits(fixture, what) {
        var width = laidOut(fixture)
        check(width > 0 && width <= preview.width, what + " wraps: " + width + " > " + preview.width + " in " + md.html)
    }

    function keepsLines() {
        laidOut("code-lines.md")
        var html = md.html
        var twoLines = preview.contentHeight
        preview.text = html.replace("first\n", "first ")
        check(twoLines > preview.contentHeight, "code lines stay apart: " + twoLines + " <= " + preview.contentHeight + " in " + html)
    }

    Component.onCompleted: {
        fits("wide-code.md", "a long line in a code block")
        fits("wide-url.md", "a long address in a paragraph")
        fits("wide-table.md", "a table with long cells")
        keepsLines()
        preview.text = "<pre>" + new Array(40).join("word ") + "</pre>"
        check(preview.contentWidth > preview.width, "unbreakable text is measured: " + preview.contentWidth)
    }
}
