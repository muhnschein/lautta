// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): a server row of Browse tells only
// how its connection stands (SPEC §15.2, NVB-4, NVB-12), never the protocol
// or the host, and each state looks different. Fails when a row's line
// says something else, shows a provider or host, or two states share a
// look.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win


    property bool scenarioRunning: true
    property int waited: 0
    property var browse

    // name: [the line's id, its engineering English, the look]
    readonly property var expected: ({
        "NAS": ["lautta-browse-server-connected", "Connected", "connected"],
        "Cabin": ["lautta-browse-server-connecting", "Connecting…", "connecting"],
        "Idle": ["lautta-browse-server-disconnected", "Not connected", "disconnected"],
        "Office": ["lautta-browse-server-failed", "Couldn't connect", "failed"],
        "Vault": ["lautta-browse-attention-auth", "Sign-in failed", "failed"],
        "Keep": ["lautta-browse-attention-auth", "Sign-in failed", "attention"],
        "Old": ["", "", "none"],
        "Quick": ["lautta-browse-server-connected", "Connected", "connected"],
        "Recent": ["lautta-browse-server-disconnected", "Not connected", "disconnected"]
    })
    // Provider labels and hosts of the rows, none of which may show.
    readonly property var hidden: ["SFTP", "SMB", "nas.home", "cabin.lan", "files.example", "kotisivu"]

    ListModel {
        id: servers
        ListElement { section: "servers"; uri: "lautta://nv-account:1/"; name: "NAS"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "connected"; connectionError: ""; colour: ""; count: -1; itemId: "nv-account:1"; place: "sftp://nas.home/"; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:2/"; name: "Cabin"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "connecting"; attention: ""; connection: "connecting"; connectionError: ""; colour: ""; count: -1; itemId: "nv-account:2"; place: ""; provider: "SMB"; host: "cabin.lan"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:3/"; name: "Idle"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "disconnected"; connectionError: ""; colour: ""; count: -1; itemId: "nv-account:3"; place: ""; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:4/"; name: "Office"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "failed"; connectionError: "Timeout"; colour: ""; count: -1; itemId: "nv-account:4"; place: ""; provider: "SMB"; host: "cabin.lan"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:5/"; name: "Vault"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "failed"; connectionError: "AuthFailed"; colour: ""; count: -1; itemId: "nv-account:5"; place: ""; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:6/"; name: "Keep"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "attention"; attention: "auth-failed"; connection: "connected"; connectionError: ""; colour: ""; count: -1; itemId: "nv-account:6"; place: ""; provider: "SMB"; host: "cabin.lan"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:7/"; name: "Old"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; count: -1; itemId: "nv-account:7"; place: ""; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-adhoc:1/"; name: "Quick"; kind: "adhoc"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "connected"; connectionError: ""; colour: ""; count: -1; itemId: "nv-adhoc:1"; place: "sftp://files.example/"; provider: "SFTP"; host: "files.example"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "sftp://files.kotisivu.fi/"; name: "Recent"; kind: "adhocRecent"; icon: "image://theme/icon-m-computer"; status: "offline"; attention: ""; connection: "disconnected"; connectionError: ""; colour: ""; count: -1; itemId: "nv-adhoc:9"; place: "sftp://files.kotisivu.fi/"; provider: "SFTP"; host: ""; free: -1; total: -1; fs: "" }
    }

    // Loaded rather than declared: a page item on its own gets no size
    // here, and its list would make no delegates.
    initialPage: Component {
        Loader {
            source: Qt.resolvedUrl("../../qml/pages/BrowsePage.qml")
            onLoaded: {
                item.listModel = servers
                win.browse = item
            }
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
    }

    function findAll(item, pred, out) {
        if (!item || out.indexOf(item) >= 0)
            return out
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    // The server rows by name, every one of them made.
    function serverRows() {
        // Headless, no render loop polishes the list: lay it out by hand.
        var views = findAll(win.contentItem, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i) {
            views[i].cacheBuffer = 100000
            views[i].forceLayout()
        }
        var rows = findAll(win.contentItem, function(x) { return x.serverText !== undefined && x.openMenu !== undefined }, [])
        // Headless, the page is not shown and nothing in it is visible:
        // show what holds the rows, so that only what a row hides is.
        for (var r = 0; r < rows.length; ++r) {
            for (var p = rows[r].parent; p; p = p.parent) {
                if (!p.visible)
                    p.visible = true
            }
        }
        var byName = {}
        for (var j = 0; j < rows.length; ++j) {
            var shown = texts(rows[j])
            for (var k = 0; k < shown.length; ++k) {
                if (win.expected[shown[k]] !== undefined)
                    byName[shown[k]] = rows[j]
            }
        }
        return byName
    }

    // How the row's status looks: its dot, busy indicator or warning icon.
    function look(row) {
        var parts = []
        findAll(row, function(x) {
            return x.visible && x.radius !== undefined && x.border !== undefined && x.radius > 0 && x.width < Theme.iconSizeSmall
        }, []).forEach(function(dot) { parts.push("dot " + dot.color + " " + dot.border.width) })
        findAll(row, function(x) { return x.visible && x.running !== undefined && x.size !== undefined }, [])
            .forEach(function() { parts.push("busy") })
        findAll(row, function(x) { return x.visible && x.source !== undefined && String(x.source).indexOf("icon-s-warning") >= 0 }, [])
            .forEach(function() { parts.push("warning") })
        return parts.length > 0 ? parts.join(", ") : "none"
    }

    // The texts the row shows.
    function texts(row) {
        return findAll(row, function(x) { return x.visible && x.font !== undefined && typeof x.text === "string" && x.text.length > 0 }, [])
            .map(function(label) { return label.text })
    }

    function check(rows) {
        var looks = {}
        for (var name in win.expected) {
            var want = win.expected[name]
            var row = rows[name]
            if (want[0] !== "" && row.serverText !== want[0] && row.serverText !== want[1])
                fail(name + " says " + row.serverText + ", not " + want[1])
            if (want[0] === "" && row.serverText !== "")
                fail(name + " says " + row.serverText + " without a state")
            var shown = texts(row)
            for (var i = 0; i < shown.length; ++i) {
                for (var k = 0; k < win.hidden.length; ++k) {
                    if (shown[i].indexOf(win.hidden[k]) >= 0)
                        fail(name + " shows " + shown[i])
                }
            }
            if (want[0] === "" && shown.length !== 1)
                fail(name + " shows more than its name: " + shown.join(" | "))
            var seen = look(row)
            if (looks[want[2]] === undefined)
                looks[want[2]] = seen
            else if (looks[want[2]] !== seen)
                fail(name + " looks " + seen + ", not " + looks[want[2]] + " as the other " + want[2] + " rows")
        }
        var kinds = Object.keys(looks)
        for (var a = 0; a < kinds.length; ++a) {
            for (var b = a + 1; b < kinds.length; ++b) {
                if (looks[kinds[a]] === looks[kinds[b]])
                    fail(kinds[a] + " and " + kinds[b] + " look alike: " + looks[kinds[a]])
            }
        }
        if (looks["none"] !== "none")
            fail("a row without a state shows " + looks["none"])
        console.log("server rows look: " + JSON.stringify(looks))
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            if (++win.waited > 150) {
                win.fail("the server rows never all appeared")
                win.scenarioRunning = false
                return
            }
            if (!win.browse)
                return
            var rows = serverRows()
            if (Object.keys(rows).length < Object.keys(win.expected).length)
                return
            win.check(rows)
            win.scenarioRunning = false
        }
    }
}
