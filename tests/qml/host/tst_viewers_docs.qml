// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of TextDocument and MarkdownDocument (PRV-4) on files in the
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
    MarkdownDocument { id: md }

    Component.onCompleted: {
        var uri = "lautta://user-documents/notes.txt"
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), uri), "fixture installed")
        check(!tools.installFixture("not a url", uri), "bad source refused")
        check(!tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), "lautta://nowhere/x"), "bad target refused")

        // Loading: text is normalised.
        check(!doc.loaded, "nothing loaded before a uri")
        doc.uri = uri
        check(doc.loading, "loading starts at once")
        tools.pump(300)
        check(!doc.loading && doc.loaded, "loaded")
        check(doc.text === "line one\nline two", "text normalised: " + JSON.stringify(doc.text))
        check(doc.validUtf8 && !doc.truncated, "flags")
        check(doc.size === 20, "size " + doc.size)

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
