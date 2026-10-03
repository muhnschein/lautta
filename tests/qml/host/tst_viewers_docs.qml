// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of TextDocument and MarkdownDocument (PRV-4, EDT-2, EDT-4) on
// files in the temporary home. ViewerTools.pump runs the event loop so the
// async results arrive while the test runs.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property var conflicts: []
    property var savedUris: []
    property var failures: []

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function readFile(uri) {
        var x = new XMLHttpRequest()
        x.open("GET", App.localUrl(uri), false)
        x.send()
        return x.responseText
    }

    ViewerTools { id: tools }
    TextDocument {
        id: doc
        onSaved: root.savedUris.push(uri)
        onConflicted: root.conflicts.push({ "deleted": deleted, "size": size })
        onSaveFailed: root.failures.push(kind)
    }
    TextDocument { id: missing }
    MarkdownDocument { id: md }

    Component.onCompleted: {
        var uri = "lautta://user-documents/notes.txt"
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), uri), "fixture installed")
        check(!tools.installFixture("not a url", uri), "bad source refused")
        check(!tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), "lautta://nowhere/x"), "bad target refused")

        // Loading: text is normalised, the conventions are reported.
        check(!doc.loaded && !doc.editable, "nothing loaded before a uri")
        doc.uri = uri
        check(doc.loading, "loading starts at once")
        tools.pump(300)
        check(!doc.loading && doc.loaded, "loaded")
        check(doc.text === "line one\nline two", "text normalised: " + JSON.stringify(doc.text))
        check(doc.lineEnding === "crlf", "line ending reported: " + doc.lineEnding)
        check(doc.finalNewline && !doc.bom && doc.validUtf8 && !doc.truncated, "flags")
        check(doc.size === 20, "size " + doc.size)
        check(doc.writable && doc.editable, "editable (EDT-4)")

        // Saving puts CRLF and the final newline back.
        doc.save("line one\nline two\nline three")
        check(doc.saving, "saving flag")
        tools.pump(300)
        check(root.savedUris.length === 1 && root.savedUris[0] === uri, "saved signal")
        check(readFile(uri) === "line one\r\nline two\r\nline three\r\n", "CRLF kept: " + JSON.stringify(readFile(uri)))

        // The file changed behind our back: conflict, nothing written (EDT-2).
        check(tools.installFixture(Qt.resolvedUrl("fixtures/doc.md"), uri), "file replaced")
        doc.save("mine")
        tools.pump(300)
        check(root.conflicts.length === 1 && !root.conflicts[0].deleted, "conflict reported")
        check(readFile(uri).indexOf("# Title") === 0, "the other version is untouched")
        doc.saveAsCopy("mine")
        tools.pump(300)
        check(root.savedUris.length === 2 && root.savedUris[1] === "lautta://user-documents/notes%202.txt", "copy saved: " + root.savedUris[1])
        check(readFile("lautta://user-documents/notes 2.txt") === "mine\r\n", "copy content")
        doc.saveReplace("replaced")
        tools.pump(300)
        check(readFile(uri) === "replaced\r\n", "replace writes over")

        // Errors.
        missing.uri = "lautta://user-documents/nope.txt"
        tools.pump(300)
        check(missing.errorKind === "NotFound" && !missing.loaded, "missing file: " + missing.errorKind)
        missing.save("x")
        check(root.failures.length === 0, "nothing to save without content is not an error of the facade")
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
