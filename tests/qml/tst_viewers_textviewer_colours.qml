// SPDX-License-Identifier: LGPL-2.1-or-later Silica window sits in a Qt Quick window so the list
// has a size and lays out rows.
import QtQuick 2.6
import QtQuick.Window 2.2 as QtQuick
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

QtQuick.Window {
    id: window

    property bool scenarioRunning: true
    property var __silica_applicationwindow_instance: app
    property int step: 0
    property int waited: 0
    property var page

    width: 540
    height: 960

    ViewerTools { id: tools }
    TextDocument { id: warmUp }

    Label {
        id: plain
        visible: false
        textFormat: Text.PlainText
        wrapMode: Text.WrapAtWordBoundaryOrAnywhere
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

    function showsItsLine(row, number) {
        var line = page.layout.rows[number - 1].text
        plain.width = row.width
        plain.text = line
        expect(row.contentWidth > 0 && row.lineCount === plain.lineCount
               && Math.abs(row.contentWidth - plain.contentWidth) < 1,
               "line " + number + " " + JSON.stringify(line) + " is " + row.lineCount + " lines, " + row.contentWidth
               + " wide as " + JSON.stringify(row.text) + ", " + plain.lineCount + " and " + plain.contentWidth
               + " as plain text")
    }

    function checkWrapped(shown) {
        console.log("palette " + page.colours)
        var comment = colourOf("comment")
        expect(comment !== "none", "a comment colour in " + page.colours)
        expect(shown["2"].text.indexOf('<font color="' + comment + '">') === 0,
               "the comment is coloured: " + shown["2"].text)
        expect(shown["2"].lineCount > 1, "the comment wraps: " + shown["2"].lineCount)
        for (var n = 1; n <= 2; ++n)
            showsItsLine(shown[String(n)], n)
    }

    function checkScript(shown) {
        expect(shown["3"].text.indexOf('<font color="' + colourOf("function") + '">echo</font>') === 0,
               "echo is coloured: " + shown["3"].text)
        expect(shown["3"].text.indexOf('<font color="' + colourOf("string") + '">') > 0,
               "the string is coloured: " + shown["3"].text)
        for (var n = 3; n <= 4; ++n)
            showsItsLine(shown[String(n)], n)
    }

    function scrollTo(index) {
        var lists = findAll(page, function(x) { return x.positionViewAtIndex !== undefined }, [])
        lists[0].positionViewAtIndex(index, ListView.Beginning)
    }

    function checkTabbed(shown) {
        var tabbed = '&nbsp;&nbsp;&nbsp;&nbsp;<font color="' + colourOf("function") + '">echo</font>'
        expect(shown["2"].text.indexOf(tabbed) === 0, "the row with a tab: " + shown["2"].text)
    }

    function checkLong(shown) {
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
                if (!shown["2"] || shown["2"].text.indexOf("<font") < 0)
                    return
                window.checkWrapped(shown)
                window.scrollTo(2)
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
                if (window.waited < 10)
                    return
                window.checkMarkup(window.rows())
                window.page = app.pageStack.replace(rowsPage, {}, PageStackAction.Immediate)
                break
            case 3:
                if (!shown["2"] || shown["2"].text.indexOf("<font") < 0)
                    return
                window.checkTabbed(shown)
                window.scrollTo(3)
                break
            case 4:
                if (!shown[""] || shown[""].text.indexOf("<font") < 0)
                    return
                window.checkLong(shown)
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
        id: markupPage
        TextViewer { uri: "lautta://user-documents/markup.xyz" }
    }

    Component {
        id: rowsPage
        TextViewer { uri: "lautta://user-documents/rows.sh" }
    }
}
