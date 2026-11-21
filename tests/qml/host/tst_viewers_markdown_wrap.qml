// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root
    property bool reachedEnd

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function html(d) {
        var out = []
        for (var i = 0; i < d.blockCount; ++i)
            out.push(d.block(i))
        check(d.block(d.blockCount) === "" && d.block(-1) === "", "no block past the ends")
        return out.join("")
    }

    function settle(o) {
        for (var i = 0; i < 1200 && o.loading; ++i)
            tools.pump(25)
        tools.pump(25)
    }

    // Every styled block on: colours must not stop wrapping.
    readonly property string style: "md-heading=#74d4ff,md-accent=#74d4ff,md-secondary=#99ffffff,md-tint=#2174d4ff,md-rule=#47ffffff,md-gap=24,md-code=20,keyword=#ff9be0"

    ViewerTools { id: tools }
    MarkdownDocument { id: md; colours: root.style }

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
        check(md.errorKind === "" && html(md).length > 0, fixture + " rendered: " + md.errorKind)
        preview.text = html(md)
        return preview.contentWidth
    }

    function fits(fixture, what) {
        var width = laidOut(fixture)
        check(width > 0 && width <= preview.width, what + " wraps: " + width + " > " + preview.width + " in " + html(md))
    }

    function keepsLines() {
        laidOut("code-lines.md")
        var text = html(md)
        var twoLines = preview.contentHeight
        preview.text = text.replace("first\n", "first ")
        check(twoLines > preview.contentHeight, "code lines stay apart: " + twoLines + " <= " + preview.contentHeight + " in " + text)
    }

    function proseKeepsLines() {
        fits("wide-prose.txt", "a long address in plain text")
        check(html(md).indexOf("<b>") < 0 && html(md).indexOf("&lt;b&gt;not bold") > 0, "plain text is not markup: " + html(md))
        var text = html(md)
        var height = preview.contentHeight
        preview.text = text.replace(/\n/g, " ")
        check(height > preview.contentHeight, "line breaks of plain text stay: " + height + " <= " + preview.contentHeight)
    }

    Component.onCompleted: {
        fits("wide-code.md", "a long line in a code block")
        fits("styled.md", "tasks, a quote and a table")
        check(html(md).indexOf("color: #74d4ff") > 0 && html(md).indexOf("bgcolor=\"#47ffffff\"") > 0, "styled with the colours given: " + html(md))
        proseKeepsLines()
        fits("wide-url.md", "a long address in a paragraph")
        fits("wide-table.md", "a table with long cells")
        keepsLines()
        preview.text = "<pre>" + new Array(40).join("word ") + "</pre>"
        check(preview.contentWidth > preview.width, "unbreakable text is measured: " + preview.contentWidth)
        reachedEnd = true
    }
}
