// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of PdfDocument and image://lautta-pdf/ (PRV-12) on files in the
// temporary home. ViewerTools.pump runs the event loop so the async results
// arrive while the test runs.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    readonly property string pdf: "lautta://user-documents/two-pages.pdf"

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function settle(item) {
        for (var i = 0; i < 40 && item.status === Image.Loading; ++i)
            tools.pump(50)
    }

    ViewerTools { id: tools }
    PdfDocument { id: doc }
    PdfDocument { id: other }

    Image { id: first; asynchronous: true; cache: false }
    Image { id: second; asynchronous: true; cache: false }

    Component.onCompleted: {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/two-pages.pdf"), pdf), "pdf fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/locked.pdf"), "lautta://user-documents/locked.pdf"), "locked fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), "lautta://user-documents/fake.pdf"), "fake fixture")

        // Pages and their sizes.
        check(!doc.loaded && doc.pageCount === 0, "nothing before a uri")
        doc.uri = pdf
        check(doc.loading, "loading starts at once")
        tools.pump(300)
        check(!doc.loading && doc.loaded, "loaded")
        check(doc.pageCount === 2, "page count " + doc.pageCount)
        check(doc.size === 640, "size " + doc.size)
        check(doc.heightFor(0, 50) === 100, "portrait page height " + doc.heightFor(0, 50))
        check(doc.heightFor(1, 300) === 150, "landscape page height " + doc.heightFor(1, 300))
        check(doc.heightFor(2, 300) === 0 && doc.heightFor(-1, 300) === 0, "no such page")
        check(doc.problem === "" && doc.errorKind === "", "no error")

        // Pages as images, at the asked width.
        first.sourceSize.width = 50
        first.source = "image://lautta-pdf/" + pdf + "?page=0"
        settle(first)
        check(first.status === Image.Ready, "page 1 rendered: " + first.status)
        check(first.implicitWidth === 50 && first.implicitHeight === 100,
              "page 1 size " + first.implicitWidth + "x" + first.implicitHeight)
        second.source = "image://lautta-pdf/" + pdf + "?page=1"
        settle(second)
        check(second.status === Image.Ready && second.implicitWidth === 1080,
              "default width " + second.implicitWidth)
        // Failing pages are checked in lautta-qt's provider test: an Image
        // in error warns, which fails this check.

        // What cannot be shown, and why.
        other.uri = "lautta://user-documents/locked.pdf"
        tools.pump(300)
        check(other.problem === "encrypted" && !other.loaded, "locked: " + other.problem)
        other.uri = "lautta://user-documents/fake.pdf"
        tools.pump(300)
        check(other.problem === "invalid" && other.pageCount === 0, "not a pdf: " + other.problem)
        other.uri = "lautta://user-documents/missing.pdf"
        tools.pump(300)
        check(other.errorKind === "NotFound" && other.problem === "", "missing: " + other.errorKind)
        other.uri = "garbage"
        check(!other.loading && !other.loaded && other.errorKind === "", "bad uri clears")
        other.uri = pdf
        other.reload()
        tools.pump(300)
        check(other.loaded && other.pageCount === 2, "reload")
    }
}
