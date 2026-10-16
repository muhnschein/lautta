// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of PdfDocument, its text search and image://lautta-pdf/
// (PRV-12) on files in the temporary home. ViewerTools.pump runs the event loop so the async results
// arrive while the test runs.
import QtQuick 2.6
import Lautta 1.0
import "../../../qml/components/viewers/PdfText.js" as PdfText

Item {
    id: root

    readonly property string pdf: "lautta://user-documents/two-pages.pdf"
    // Three pages: "Lautta" once on the first, twice on the third.
    readonly property string searchable: "lautta://user-documents/searchable.pdf"

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function settle(item) {
        for (var i = 0; i < 40 && item.status === Image.Loading; ++i)
            tools.pump(50)
    }

    // Waits for a PdfDocument to finish loading (unlocking AES-256 takes a
    // while in a debug build).
    function loaded(d) {
        tools.pump(50)
        for (var i = 0; i < 100 && d.loading; ++i)
            tools.pump(50)
    }

    // Waits for a search to finish (30 s at most).
    function searched(d) {
        for (var i = 0; i < 600 && d.searching; ++i)
            tools.pump(50)
    }

    ViewerTools { id: tools }
    PdfDocument { id: doc }
    PdfDocument { id: other }
    PdfDocument { id: found }
    PdfDocument {
        id: linked
        property var got: ({})
        onLinksReady: got[linkPage] = JSON.parse(pageLinks)
    }

    Image { id: first; asynchronous: true; cache: false }
    Image { id: second; asynchronous: true; cache: false }

    // The viewer's rules for links, the section being read and "Go to page".
    function checkTexts() {
        var web = ["https://example.org/x", "HTTP://a", "http://a/b?c=d#e"]
        for (var i = 0; i < web.length; ++i)
            check(PdfText.isWebLink(web[i]), "web link " + web[i])
        var notWeb = ["", "https://", "http://a b", "file:///etc/passwd", "javascript:alert(1)",
                      " https://a", "ftp://a", "mailto:a@b", "httpx://a"]
        for (var j = 0; j < notWeb.length; ++j)
            check(!PdfText.isWebLink(notWeb[j]), "not a web link: " + notWeb[j])
        var outline = [{ "page": 0 }, { "page": 2 }, { "page": -1 }, { "page": 2 }, { "page": 1 }, { "page": 5 }]
        var current = [[0, 0], [1, 4], [2, 3], [4, 3], [5, 5], [9, 5]]
        for (var k = 0; k < current.length; ++k) {
            var got = PdfText.currentItem(outline, current[k][0])
            check(got === current[k][1], "section of page " + current[k][0] + ": " + got)
        }
        check(PdfText.currentItem([{ "page": 3 }, { "page": -1 }], 1) === -1, "before the first section")
        check(PdfText.currentItem([], 1) === -1, "no outline")
        var typed = [["1", 0], ["12", 11], [" 7 ", 6], ["0", -1], ["13", -1], ["", -1],
                     ["-1", -1], ["2.5", -1], ["1e1", -1], ["x", -1], ["012", 11]]
        for (var m = 0; m < typed.length; ++m) {
            var page = PdfText.typedPage(typed[m][0], 12)
            check(page === typed[m][1], "typed '" + typed[m][0] + "': " + page)
        }
    }

    Component.onCompleted: {
        checkTexts()
        check(tools.installFixture(Qt.resolvedUrl("fixtures/two-pages.pdf"), pdf), "pdf fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/secret.pdf"), "lautta://user-documents/locked.pdf"), "locked fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), "lautta://user-documents/fake.pdf"), "fake fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/secret.pdf"), "lautta://user-documents/secret.pdf"), "secret fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/linked.pdf"), "lautta://user-documents/linked.pdf"), "linked fixture")

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
        loaded(other)
        check(other.problem === "encrypted" && !other.loaded, "locked: " + other.problem)
        other.uri = "lautta://user-documents/fake.pdf"
        loaded(other)
        check(other.problem === "invalid" && other.pageCount === 0, "not a pdf: " + other.problem)
        other.uri = "lautta://user-documents/missing.pdf"
        loaded(other)
        check(other.errorKind === "NotFound" && other.problem === "", "missing: " + other.errorKind)
        other.uri = "garbage"
        check(!other.loading && !other.loaded && other.errorKind === "", "bad uri clears")
        other.uri = pdf
        other.reload()
        loaded(other)
        check(other.loaded && other.pageCount === 2, "reload")

        // Reading position: none at first, then where it stopped.
        check(other.lastPage === -1, "no position yet: " + other.lastPage)
        other.rememberPage(1)
        other.rememberPage(7)
        other.rememberPage(-1)
        tools.pump(300)
        other.reload()
        loaded(other)
        check(other.lastPage === 1, "position kept: " + other.lastPage)

        // A password opens an encrypted PDF; a wrong one says so.
        other.uri = "lautta://user-documents/secret.pdf"
        loaded(other)
        check(other.problem === "encrypted" && !other.wrongPassword, "asks for a password")
        other.unlock("wrong")
        check(other.loading, "unlocking")
        loaded(other)
        check(other.problem === "encrypted" && other.wrongPassword, "wrong password: " + other.problem)
        other.unlock("sesame")
        loaded(other)
        check(other.loaded && other.pageCount === 2 && other.problem === "" && !other.wrongPassword,
              "unlocked: " + other.problem)
        other.reload()
        loaded(other)
        check(other.problem === "encrypted" && !other.wrongPassword, "the password is not kept")

        // Outline and links; only page and web links are given.
        check(JSON.stringify(JSON.parse(doc.outline)) === "[]", "no outline: " + doc.outline)
        linked.requestLinks(0)
        tools.pump(100)
        check(linked.got[0] === undefined, "no links before loading")
        linked.uri = "lautta://user-documents/linked.pdf"
        loaded(linked)
        check(JSON.stringify(JSON.parse(linked.outline))
              === '[{"level":0,"page":0,"title":"Start"},{"level":1,"page":1,"title":"Later"}]',
              "outline: " + linked.outline)
        linked.requestLinks(0)
        linked.requestLinks(1)
        linked.requestLinks(2)
        linked.requestLinks(-1)
        tools.pump(300)
        var links = linked.got[0] || []
        check(links.length === 2, "two links: " + JSON.stringify(links))
        check(links.length === 2 && links[0].page === 1 && links[0].top === 0.5 && links[0].uri === ""
              && links[0].x === 0 && links[0].y === 0 && links[0].width === 0.5 && links[0].height === 0.5,
              "page link: " + JSON.stringify(links[0]))
        check(links.length === 2 && links[1].page === -1 && links[1].top === -1
              && links[1].uri === "https://example.org/x" && links[1].x === 0.5 && links[1].y === 0.5,
              "web link: " + JSON.stringify(links[1]))
        check(JSON.stringify(linked.got[1]) === "[]", "no links on page 2")
        check(linked.got[2] === undefined && linked.got[-1] === undefined, "no such page")
        linked.got = {}
        linked.requestLinks(0)
        linked.uri = pdf
        loaded(linked)
        tools.pump(200)
        check(linked.got[0] === undefined, "links of the file before are dropped")

        searchTests()
    }

    function searchTests() {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/searchable.pdf"), searchable), "searchable fixture")
        found.uri = searchable
        loaded(found)
        check(found.loaded && found.pageCount === 3, "searchable pdf")
        check(!found.searching && found.hitCount === 0 && found.currentHit === 0 && found.currentHitPage === -1,
              "no search yet")

        // Every page is searched; the hit shown first is the first at or
        // after the page being read, numbered in page order.
        found.search("lautta", 1)
        check(found.searching, "searching starts at once")
        searched(found)
        check(!found.searching && found.searchedPages === 3, "searched every page: " + found.searchedPages)
        check(found.hitCount === 3, "hits " + found.hitCount)
        check(found.currentHit === 2 && found.currentHitPage === 2,
              "first hit from page 2: " + found.currentHit + " on " + found.currentHitPage)
        var areas = JSON.parse(found.hitsJson(2))
        check(areas.length === 2 && areas[0].current && !areas[1].current, "page 3: " + found.hitsJson(2))
        var a = areas[0]
        check(a.x > 0.05 && a.x < 0.1 && a.width > 0.1 && a.width < 0.2 && a.y > 0.09 && a.y < 0.15 && a.height > 0.02,
              "where the hit is: " + JSON.stringify(a))
        check(found.hitsJson(1) === "[]" && found.hitsJson(-1) === "[]" && found.hitsJson(9) === "[]", "no hits there")

        // Next and previous go round.
        var revision = found.hitsRevision
        found.nextHit()
        check(found.currentHit === 3 && found.hitsRevision !== revision, "next: " + found.currentHit)
        check(JSON.parse(found.hitsJson(2))[1].current, "the hit shown is marked")
        found.nextHit()
        check(found.currentHit === 1 && found.currentHitPage === 0, "after the last the first: " + found.currentHit)
        check(JSON.parse(found.hitsJson(0))[0].current && !JSON.parse(found.hitsJson(2))[1].current, "marked once")
        found.previousHit()
        check(found.currentHit === 3 && found.currentHitPage === 2, "before the first the last: " + found.currentHit)

        // Nothing found in the whole document.
        found.search("ferry boat", 0)
        searched(found)
        check(found.searchedPages === 3 && found.hitCount === 0 && found.currentHit === 0 && found.currentHitPage === -1,
              "not found: " + found.searchedPages + " " + found.hitCount)

        // A blank query searches nothing; stopping forgets the hits.
        found.search("lautta", 0)
        searched(found)
        found.search("  ", 0)
        check(!found.searching && found.searchedPages === 0 && found.hitCount === 0, "blank query")
        found.search("lautta", 0)
        found.stopSearch()
        check(!found.searching && found.hitCount === 0, "stopped")
        tools.pump(300)
        check(found.searchedPages === 0 && found.hitCount === 0, "late results are ignored: " + found.searchedPages)

        // Only the last search counts.
        found.search("river", 0)
        found.search("the end", 0)
        searched(found)
        check(found.hitCount === 1 && found.currentHitPage === 2 && found.searchedPages === 3,
              "replaced search: " + found.hitCount + " on " + found.currentHitPage)

        // Another document forgets the hits; one not loaded is not searched.
        found.uri = pdf
        check(found.hitCount === 0 && found.searchedPages === 0, "another document")
        found.uri = "lautta://user-documents/missing.pdf"
        loaded(found)
        found.search("lautta", 0)
        check(!found.searching, "nothing to search")
    }
}
