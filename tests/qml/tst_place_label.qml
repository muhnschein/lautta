// SPDX-License-Identifier: LGPL-2.1-or-later
// PlaceLabel on the target (real Silica, §15.5): a line naming a server's
// place has the small server icon right before the name, in the line's
// colour, and a local place has none; text before the name stays before
// the icon; a right-aligned line (a section header) ends at its right edge
// with the icon against the name; too long a line keeps the icon and the
// name inside the line. The search page marks its folder headers on a
// server only. Fails naming the line that is laid out otherwise. Headless,
// nothing is on screen: the lines are checked by their layout, not by
// `visible`.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"
import "../../qml/pages"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int waited: 0

    // Straight in the window: ids inside a page component are not seen here.
    Column {
        width: 540

        PlaceLabel {
            id: remoteLine
            width: parent.width
            text: "NAS › /srv/photos"
            remote: true
            color: "#123456"
        }
        PlaceLabel {
            id: localLine
            width: parent.width
            text: "Documents › Uni"
            remote: false
        }
        PlaceLabel {
            id: sentenceLine
            width: parent.width
            before: "Copied 4 items to "
            text: "NAS"
            remote: true
        }
        PlaceLabel {
            id: headerLine
            width: parent.width
            text: "photos › 2024"
            remote: true
            horizontalAlignment: Text.AlignRight
        }
        PlaceLabel {
            id: longLine
            width: 200
            before: "Copying a-very-long-file-name-that-does-not-fit.mp4 to "
            text: "NAS"
            remote: true
        }
        PlaceLabel {
            id: emptyLine
            width: parent.width
            text: ""
            remote: true
        }
    }

    SearchPage {
        id: remoteSearch
        rootUri: "lautta://nv-account:1/srv"
    }
    SearchPage {
        id: localSearch
        rootUri: "lautta://user-documents/"
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
    }

    function partsOf(line) {
        var found = { "labels": [] }
        var kids = line.children
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].objectName === "remoteIcon")
                found.icon = kids[i]
            else if (kids[i].text !== undefined)
                found.labels.push(kids[i])
        }
        // Left to right: the text before the name, then the name.
        found.lead = found.labels[0]
        found.name = found.labels[1]
        return found
    }

    function check() {
        var r = partsOf(remoteLine)
        expect(remoteLine.iconShown && /\/icons\/server\.svg$/.test(String(r.icon.source))
               && r.icon.status === Image.Ready,
               "a server's place has the server icon")
        expect(r.icon.x + r.icon.width <= r.name.x && r.icon.x === 0, "the icon stands right before the name")
        expect(Qt.colorEqual(r.icon.color, remoteLine.color) && Qt.colorEqual(r.name.color, remoteLine.color),
               "icon and name are in the line's colour")
        var l = partsOf(localLine)
        expect(!localLine.iconShown && l.name.x === 0, "a local place has no icon")

        var s = partsOf(sentenceLine)
        expect(s.lead.text === "Copied 4 items to " && s.lead.x === 0 && s.lead.width > 0, "the text before comes first")
        expect(sentenceLine.iconShown && s.icon.x >= s.lead.width && s.icon.x + s.icon.width <= s.name.x
               && s.name.text === "NAS", "then the icon and the name")

        var h = partsOf(headerLine)
        expect(Math.round(h.name.x + h.name.width) === Math.round(headerLine.width),
               "a right-aligned line ends at its edge: " + (h.name.x + h.name.width) + " of " + headerLine.width)
        expect(headerLine.iconShown && h.icon.x + h.icon.width <= h.name.x && h.name.x - h.icon.x < h.icon.width * 2,
               "with the icon against the name")

        var g = partsOf(longLine)
        expect(g.lead.width < longLine.width && longLine.iconShown && g.name.x + g.name.width <= longLine.width + 0.5,
               "too long a line keeps the icon and the name inside: " + g.lead.width + ", " + g.name.x + "+" + g.name.width)
        expect(!emptyLine.iconShown, "no icon without a name")

        expect(remoteSearch.remote && !localSearch.remote, "the search page marks folder headers on a server only")
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            if (++win.waited < 3)
                return
            win.check()
            win.scenarioRunning = false
        }
    }
}
