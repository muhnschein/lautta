// SPDX-License-Identifier: LGPL-2.1-or-later
// TextViewer's colours on the target (PRV-13, real Silica and Qt 5.6's
// StyledText): a shell script's rows are coloured with the ambience's
// palette, wrapped or not, a file of unknown type gets plain rows, and
// markup in a file shows as its own characters: each row is exactly as wide
// as its text laid out as plain text, so no tag, entity or space of the file
// was taken as markup or lost. Rows that tabs and long lines make (PRV-4)
// have the colours of their part of the line.
//
// The Silica window sits in a Qt Quick window so the list has a size and
// lays out its rows.
import QtQuick 2.6
import QtQuick.Window 2.2 as QtQuick
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

QtQuick.Window {
    id: window

    property bool scenarioRunning: true
    // What Silica's pages look up in the window they are in.
    property var __silica_applicationwindow_instance: app
    property int step: 0
    property int waited: 0
    property var page

    width: 540
    height: 960

    ViewerTools { id: tools }
    TextDocument { id: warmUp }

    // A row's line as plain text, to measure the row against.
    Label {
        id: plain
        visible: false
        textFormat: Text.PlainText
        font.family: "monospace"
        font.pixelSize: Theme.fontSizeExtraSmall
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function expect(ok, what) {
        if (!ok)
            fail(what)
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

    // The text label of each row on screen by line number (from 1); the
    // row's other label is its number.
    function rows() {
        var lists = findAll(page, function(x) { return x.forceLayout !== undefined }, [])
        if (lists.length > 0)
            lists[0].forceLayout()
        var labels = findAll(page, function(x) { return x.textFormat === Text.StyledText && x.font.family === "monospace" }, [])
        var out = {}
        for (var i = 0; i < labels.length; ++i) {
            var siblings = labels[i].parent.children
            for (var k = 0; k < siblings.length; ++k) {
                if (siblings[k] !== labels[i] && siblings[k].horizontalAlignment === Text.AlignRight)
                    out[siblings[k].text] = labels[i]
            }
        }
        return out
    }

    function colourOf(name) {
        var match = new RegExp("(^|,)" + name + "=(#[0-9a-f]+)").exec(page.colours)
        return match ? match[2] : "none"
    }

    // The row of line `number` shows exactly the characters of the line.
    function showsItsLine(row, number) {
        var line = page.layout.rows[number - 1].text
        plain.text = line
        expect(row.implicitWidth > 0 && Math.abs(row.implicitWidth - plain.implicitWidth) < 1,
               "line " + number + " " + JSON.stringify(line) + " is " + row.implicitWidth + " wide as " + JSON.stringify(row.text)
               + ", " + plain.implicitWidth + " as plain text")
    }

    // Wrapped (the page is small, and the rows wrap to several lines: the
    // first two are on screen).
    function checkWrapped(shown) {
        console.log("palette " + page.colours)
        var comment = colourOf("comment")
        expect(page.wrap, "wrapped by default")
        expect(comment !== "none", "a comment colour in " + page.colours)
        expect(shown["2"].text.indexOf('<font color="' + comment + '">') === 0,
               "the comment is coloured: " + shown["2"].text)
        expect(shown["2"].lineCount > 1, "the comment wraps: " + shown["2"].lineCount)
    }

    function checkScript(shown) {
        expect(!page.wrap, "unwrapped")
        expect(shown["3"].text.indexOf('<font color="' + colourOf("function") + '">echo</font>') === 0,
               "echo is coloured: " + shown["3"].text)
        expect(shown["3"].text.indexOf('<font color="' + colourOf("string") + '">') > 0,
               "the string is coloured: " + shown["3"].text)
        for (var n = 1; n <= 4; ++n)
            showsItsLine(shown[String(n)], n)
    }

    // A row with a tab, and the second row of a long line.
    function checkRows(shown) {
        var tabbed = '&nbsp;&nbsp;&nbsp;&nbsp;<font color="' + colourOf("function") + '">echo</font>'
        expect(shown["2"].text.indexOf(tabbed) === 0, "the row with a tab: " + shown["2"].text)
        var rest = '<font color="' + colourOf("comment") + '"># long</font>'
        expect(shown[""] !== undefined && shown[""].text.indexOf(rest) > 0, "the long line's second row: "
               + (shown[""] ? shown[""].text.slice(-80) : "not shown"))
    }

    function checkMarkup(shown) {
        var row = shown["1"]
        expect(row.text.indexOf("<font") < 0 && row.text.indexOf("<img") < 0 && row.text.indexOf("<b>") < 0,
               "an unknown type has plain rows: " + row.text)
        var line = page.layout.rows[0].text
        expect(line.indexOf("<img") === 0, "the fixture is markup: " + line)
        showsItsLine(row, 1)
    }

    Timer {
        interval: 100
        repeat: true
        running: window.scenarioRunning
        onTriggered: {
            var shown = window.page ? window.rows() : {}
            if (++window.waited > 300)
                return window.fail("step " + window.step + " timed out: rows "
                                   + JSON.stringify(window.page.layout.rows.map(function(r) { return r.text.slice(0, 80) }))
                                   + ", rows " + JSON.stringify(Object.keys(shown).map(function(n) { return n + ": " + shown[n].text })))
            switch (window.step) {
            case 0:
                // The colours may come after the text.
                if (!shown["2"] || shown["2"].text.indexOf("<font") < 0)
                    return
                window.checkWrapped(shown)
                // A new page: the list does not lay its rows out again here
                // when they change height.
                window.page = app.pageStack.replace(unwrappedPage, {}, PageStackAction.Immediate)
                break
            case 1:
                if (!shown["4"] || shown["3"].text.indexOf("<font") < 0)
                    return
                window.checkScript(shown)
                window.page = app.pageStack.replace(markupPage, {}, PageStackAction.Immediate)
                break
            case 2:
                if (!shown["1"])
                    return
                // A plain file gets no colours later either.
                if (window.waited < 10)
                    return
                window.checkMarkup(window.rows())
                window.page = app.pageStack.replace(rowsPage, {}, PageStackAction.Immediate)
                break
            case 3:
                if (!shown["3"] || !shown[""] || shown[""].text.indexOf("<font") < 0)
                    return
                window.checkRows(shown)
                window.scenarioRunning = false
                break
            }
            window.step++
            window.waited = 0
        }
    }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/script.sh"), "lautta://user-documents/script.sh")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/markup.xyz"), "lautta://user-documents/markup.xyz")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/rows.sh"), "lautta://user-documents/rows.sh")
        // The emulated target compiles a grammar's patterns many times
        // slower than a phone does, near the colours' time limit (PRV-13):
        // colour a script once first, so the viewer's colours come in time.
        warmUp.uri = "lautta://user-documents/script.sh"
        var waited = 0
        for (; waited < 300 && (warmUp.loading || !warmUp.highlighted); ++waited)
            tools.pump(100)
        console.log("the grammar took " + waited * 100 + " ms, coloured: " + warmUp.highlighted)
        page = app.pageStack.push(scriptPage, {}, PageStackAction.Immediate)
    }

    ApplicationWindow {
        id: app
    }

    Component {
        id: scriptPage
        TextViewer { uri: "lautta://user-documents/script.sh" }
    }

    Component {
        id: unwrappedPage
        TextViewer { uri: "lautta://user-documents/script.sh"; wrap: false }
    }

    Component {
        id: markupPage
        TextViewer { uri: "lautta://user-documents/markup.xyz"; wrap: false }
    }

    Component {
        id: rowsPage
        TextViewer { uri: "lautta://user-documents/rows.sh"; wrap: false }
    }
}
