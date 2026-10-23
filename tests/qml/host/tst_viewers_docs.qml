// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of TextDocument (PRV-4, colours PRV-13) and MarkdownDocument on files in the
// temporary home. ViewerTools.pump runs the event loop so the async results
// arrive while the test runs.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    ViewerTools { id: tools }
    TextDocument { id: doc }
    TextDocument { id: missing }
    TextDocument { id: coloured }
    MarkdownDocument { id: md }

    Component.onCompleted: {
        var uri = "lautta://user-documents/notes.txt"
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), uri), "fixture installed")
        check(!tools.installFixture("not a url", uri), "bad source refused")
        check(!tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), "lautta://nowhere/x"), "bad target refused")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/script.sh"), "lautta://user-documents/script.sh"), "script installed")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/markup.xyz"), "lautta://user-documents/markup.xyz"), "markup installed")

        // Loading: text is normalised.
        check(!doc.loaded, "nothing loaded before a uri")
        doc.uri = uri
        check(doc.loading, "loading starts at once")
        tools.pump(300)
        check(!doc.loading && doc.loaded, "loaded")
        check(doc.text === "line one\nline two", "text normalised: " + JSON.stringify(doc.text))
        check(doc.validUtf8 && !doc.truncated, "flags")
        check(doc.size === 20, "size " + doc.size)
        check(!doc.highlighted && doc.syntax === "", "plain text has no colours: " + doc.syntax)
        check(doc.styledLine(1, "keyword=#ff0000") === "line two", "a plain line: " + doc.styledLine(1, ""))

        // Colours (PRV-13), which may come after the text.
        coloured.uri = "lautta://user-documents/script.sh"
        for (var waited = 0; waited < 100 && !coloured.highlighted; ++waited)
            tools.pump(100)
        check(coloured.loaded && coloured.highlighted, "a shell script is coloured")
        check(coloured.syntax === "Bourne Again Shell (bash)", "by its name: " + coloured.syntax)
        var comment = coloured.styledLine(1, "comment=#123456")
        check(comment === '<font color="#123456"># &lt;img src=&quot;http://example.org/x.png&quot;&gt; &amp;lt;b&amp;gt;</font>',
              "the comment is coloured and escaped: " + comment)
        var echo = coloured.styledLine(2, "")
        check(echo.indexOf("<") < 0 && echo.indexOf("&lt;b&gt;bold?&lt;/b&gt; &amp;amp;") > 0, "no colours, still escaped: " + echo)
        var indented = coloured.styledLine(3, "")
        check(indented === '&nbsp;&nbsp;if [ &quot;$x&quot; ]; then&nbsp;&nbsp;echo &quot;a&nbsp;&nbsp;b&quot;; fi', "whitespace is kept: " + indented)
        check(coloured.styledLine(4, "") === "" && coloured.styledLine(-1, "") === "", "no line past either end")
        coloured.uri = "lautta://user-documents/markup.xyz"
        tools.pump(500)
        check(coloured.loaded && !coloured.highlighted && coloured.syntax === "", "an unknown type is plain")
        var markup = coloured.styledLine(0, "keyword=#ff0000")
        check(markup === "&lt;img src=&quot;image://lautta-thumb/x&quot;&gt; &amp;lt;b&amp;gt; &amp; &lt;b&gt;bold&lt;/b&gt; &amp;nbsp;&lt;a href=&quot;x&quot;&gt;link&lt;/a&gt;",
              "markup in a file is text: " + markup)

        // Errors.
        missing.uri = "lautta://user-documents/nope.txt"
        tools.pump(300)
        check(missing.errorKind === "NotFound" && !missing.loaded, "missing file: " + missing.errorKind)
        var invalid = Qt.createQmlObject('import Lautta 1.0; TextDocument { uri: "garbage" }', root)
        check(!invalid.loading && !invalid.loaded, "bad uri is ignored")

        // Markdown.
        check(tools.installFixture(Qt.resolvedUrl("fixtures/doc.md"), "lautta://user-documents/doc.md"), "md fixture")
        md.uri = "lautta://user-documents/doc.md"
        check(md.loading, "markdown loading")
        tools.pump(300)
        check(md.html.indexOf("Title") >= 0 && md.html.indexOf("<em>emphasis</em>") >= 0, "rendered: " + md.html)
        check(md.html.indexOf("<script") < 0 && md.html.indexOf("&lt;script&gt;") >= 0, "raw html is shown as text")
        check(md.errorKind === "" && !md.truncated, "no error")
        md.uri = "lautta://user-documents/nope.md"
        tools.pump(300)
        check(md.errorKind === "NotFound", "markdown error " + md.errorKind)
    }
}
