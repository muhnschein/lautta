// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int waited: 0
    property bool pulseChecked: false
    property var browse

    readonly property var expected: ({
        "NAS": "on",
        "Cabin": "busy",
        "Idle": "off",
        "Office": "failed",
        "Vault": "warning",
        "Keep": "warning",
        "Old": "none",
        "Quick": "on",
        "Recent": "off"
    })
    readonly property var hidden: ["SFTP", "SMB", "nas.home", "cabin.lan", "files.example", "kotisivu"]

    ListModel {
        id: servers
        ListElement { section: "servers"; uri: "lautta://nv-account:1/"; name: "NAS"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "connected"; connectionError: ""; colour: ""; itemId: "nv-account:1"; place: "sftp://nas.home/"; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:2/"; name: "Cabin"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "connecting"; attention: ""; connection: "connecting"; connectionError: ""; colour: ""; itemId: "nv-account:2"; place: ""; provider: "SMB"; host: "cabin.lan"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:3/"; name: "Idle"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "disconnected"; connectionError: ""; colour: ""; itemId: "nv-account:3"; place: ""; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:4/"; name: "Office"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "failed"; connectionError: "Timeout"; colour: ""; itemId: "nv-account:4"; place: ""; provider: "SMB"; host: "cabin.lan"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:5/"; name: "Vault"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "failed"; connectionError: "AuthFailed"; colour: ""; itemId: "nv-account:5"; place: ""; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:6/"; name: "Keep"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "attention"; attention: "auth-failed"; connection: "connected"; connectionError: ""; colour: ""; itemId: "nv-account:6"; place: ""; provider: "SMB"; host: "cabin.lan"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:7/"; name: "Old"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "nv-account:7"; place: ""; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-adhoc:1/"; name: "Quick"; kind: "adhoc"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "connected"; connectionError: ""; colour: ""; itemId: "nv-adhoc:1"; place: "sftp://files.example/"; provider: "SFTP"; host: "files.example"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "sftp://files.kotisivu.fi/"; name: "Recent"; kind: "adhocRecent"; icon: "image://theme/icon-m-computer"; status: "offline"; attention: ""; connection: "disconnected"; connectionError: ""; colour: ""; itemId: "nv-adhoc:9"; place: "sftp://files.kotisivu.fi/"; provider: "SFTP"; host: ""; free: -1; total: -1; fs: "" }
    }

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

    function serverRows() {
        // No render loop headless: lay out by hand.
        var views = findAll(win.contentItem, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i) {
            views[i].cacheBuffer = 100000
            views[i].forceLayout()
        }
        var rows = findAll(win.contentItem, function(x) { return x.statusIndicator !== undefined && x.openMenu !== undefined }, [])
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

    function icons(row) {
        return findAll(row, function(x) { return x.visible && x.objectName === "statusDot" }, [])
    }

    function appearance(icon) {
        return "dot " + icon.color + (icon.parent.indicator === "busy" ? " pulsing" : "")
    }

    function stateOf(icon) {
        var look = appearance(icon)
        var states = {}
        states["dot " + Theme.highlightColor] = "on"
        states["dot " + Theme.primaryColor] = "off"
        states["dot " + Theme.primaryColor + " pulsing"] = "busy"
        states["dot " + Theme.errorColor] = "failed"
        states["dot " + icon.parent.warningColor] = "warning"
        return states[look] || ""
    }

    function texts(row) {
        return findAll(row, function(x) { return x.visible && x.font !== undefined && typeof x.text === "string" && x.text.length > 0 }, [])
            .map(function(label) { return label.text })
    }

    function rightEdge(item, row, width) {
        return Math.round(item.mapToItem(row, width, 0).x)
    }

    function checkPlace(name, row) {
        var slot = findAll(row, function(x) { return x.indicator !== undefined && x.dotSize !== undefined }, [])[0]
        var edge = Math.round(row.width - Theme.horizontalPageMargin)
        if (rightEdge(slot, row, slot.width) !== edge)
            fail(name + "'s icon ends at " + rightEdge(slot, row, slot.width) + ", the margin is at " + edge)
        var icon = icons(row)[0]
        var end = (icon.width + slot.dotSize) / 2
        if (rightEdge(icon, row, end) !== edge)
            fail(name + "'s " + icon.objectName + " ends at " + rightEdge(icon, row, end) + ", the margin is at " + edge)
        var middle = icon.mapToItem(row, 0, icon.height / 2).y
        if (Math.abs(middle - row.contentHeight / 2) > 1)
            fail(name + "'s icon is at " + middle + ", the middle of the row is " + row.contentHeight / 2)
        var label = findAll(row, function(x) { return x.visible && x.font !== undefined && x.text === name }, [])[0]
        if (!label || rightEdge(label, row, label.width) > slot.mapToItem(row, 0, 0).x)
            fail(name + "'s name does not end before its icon")
    }

    function check(rows) {
        var looks = {}
        for (var name in win.expected) {
            var want = win.expected[name]
            var row = rows[name]
            var shown = texts(row)
            if (shown.length !== 1 || shown[0] !== name)
                fail(name + " shows " + shown.join(" | ") + ", not only its name")
            for (var i = 0; i < shown.length; ++i) {
                for (var k = 0; k < win.hidden.length; ++k) {
                    if (shown[i].indexOf(win.hidden[k]) >= 0)
                        fail(name + " shows " + shown[i])
                }
            }
            var shownIcons = icons(row)
            if (want === "none") {
                if (shownIcons.length !== 0)
                    fail(name + " shows an icon without a state: " + shownIcons.map(appearance).join(", "))
                continue
            }
            if (shownIcons.length !== 1) {
                fail(name + " shows " + shownIcons.length + " icons: " + shownIcons.map(appearance).join(", "))
                continue
            }
            var seen = stateOf(shownIcons[0])
            if (seen !== want)
                fail(name + " shows " + appearance(shownIcons[0]) + " (" + (seen || "no known state") + "), not " + want)
            checkPlace(name, row)
            looks[want] = appearance(shownIcons[0])
        }
        var states = Object.keys(looks)
        for (var a = 0; a < states.length; ++a) {
            for (var b = a + 1; b < states.length; ++b) {
                if (looks[states[a]] === looks[states[b]])
                    fail(states[a] + " and " + states[b] + " look alike: " + looks[states[a]])
            }
        }
        console.log("server rows look: " + JSON.stringify(looks))
    }

    function checkHighlighted(row) {
        var dot = icons(row)[0]
        if (String(dot.color) !== String(Theme.highlightColor))
            fail("a highlighted row's dot is " + dot.color + ", not " + Theme.highlightColor)
    }

    function slotOf(row) {
        return findAll(row, function(x) { return x.indicator !== undefined && x.dotSize !== undefined }, [])[0]
    }

    function pulsingOf(row) {
        var slot = slotOf(row)
        for (var i = 0; i < slot.data.length; ++i) {
            if (slot.data[i].objectName === "statusPulse")
                return slot.pulsing && slot.data[i].running
        }
        return false
    }

    function checkPulse(rows) {
        for (var name in win.expected) {
            if (win.expected[name] !== "none")
                slotOf(rows[name]).appActive = true
        }
        for (name in win.expected) {
            if (win.expected[name] !== "none" && pulsingOf(rows[name]) !== (win.expected[name] === "busy"))
                fail(name + (pulsingOf(rows[name]) ? " pulses" : " does not pulse") + " as " + win.expected[name])
        }
        servers.setProperty(1, "connection", "connected")
        servers.setProperty(1, "status", "ready")
    }

    function checkConnected(row) {
        var dot = icons(row)[0]
        if (!dot || stateOf(dot) !== "on")
            fail("Cabin, connected, shows " + (dot ? appearance(dot) : "no dot"))
        else if (pulsingOf(row) || dot.opacity !== 1.0)
            fail("Cabin pulses after it connected")
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
            if (!win.pulseChecked) {
                win.check(rows)
                rows["Idle"].highlighted = true
                win.checkHighlighted(rows["Idle"])
                rows["Idle"].highlighted = false
                win.checkPulse(rows)
                win.pulseChecked = true
                return
            }
            if (win.icons(rows["Cabin"]).length === 0 || win.stateOf(win.icons(rows["Cabin"])[0]) === "busy")
                return
            win.checkConnected(rows["Cabin"])
            win.scenarioRunning = false
        }
    }
}
