// SPDX-License-Identifier: LGPL-2.1-or-later
// Browse rows on the target (real Silica, SPEC §15.2, §15.5): a folder row
// shows no item count; a server row is as tall as a folder row, its status
// at the right margin in the small font; a favourite on a server has the
// server icon before its place and a local one has none; a drive that is not
// mounted is as tall as a mounted volume, with its size below its name and
// "Not mounted" on the right like a server's status (LOC-3); and nothing is
// attached to the page to swipe to (Activity is in the pulley). Fails when
// a count comes back, server rows grow a second line again, the status
// leaves the right margin, the icon marks the wrong place or Activity is
// attached again.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int waited: 0
    property var browse

    initialPage: Component {
        BrowsePage {
            id: browsePage

            Component.onCompleted: win.browse = browsePage
            // The Documents row still has a count in the model: the page
            // shows none.
            listModel: ListModel {
                ListElement { section: "favourites"; uri: "lautta://nv-account:1/srv/photos"; name: "Photos"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "1"; place: "NAS › /srv/photos"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
                ListElement { section: "favourites"; uri: "lautta://user-documents/Uni"; name: "Thesis"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "2"; place: "Documents › Uni"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
                ListElement { section: "device"; uri: "lautta://user-documents/"; name: "Documents"; kind: "folder"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; count: 214; itemId: "user-documents"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
                ListElement { section: "volumes"; uri: "lautta://vol-SD/"; name: "SD card"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "vol-SD"; place: ""; provider: ""; host: ""; free: 62500000000; total: 128000000000; fs: "exFAT"; drive: "mmcblk1p1" }
                ListElement { section: "volumes"; uri: ""; name: "STICK"; kind: "volume"; icon: "image://theme/icon-m-usb"; status: "unmounted"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "vol-STICK"; place: ""; provider: ""; host: ""; free: -1; total: 8000000000; fs: ""; drive: "sda1" }
                ListElement { section: "servers"; uri: "lautta://nv-account:1/"; name: "NAS"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "connected"; connectionError: ""; colour: ""; itemId: "nv-account:1"; place: "sftp://nas.home/"; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
                ListElement { section: "servers"; uri: "lautta://nv-account:2/"; name: "Office"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "attention"; attention: "auth-failed"; connection: "failed"; connectionError: "AuthFailed"; colour: ""; itemId: "nv-account:2"; place: ""; provider: "SMB"; host: ""; free: -1; total: -1; fs: "" }
                ListElement { section: "servers"; uri: "lautta://nv-account:3/"; name: "Old"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "nv-account:3"; place: ""; provider: "SFTP"; host: ""; free: -1; total: -1; fs: "" }
            }
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
    }

    function findAll(item, pred, out) {
        if (!item)
            return out
        // A flickable's contentItem is one of its children too.
        if (pred(item) && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    // The delegate of the row named `name`, laid out.
    function rowOf(name) {
        var views = findAll(browse, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        if (views.length === 0)
            return null
        var list = views[0]
        list.cacheBuffer = 100000
        list.forceLayout()
        for (var i = 0; i < list.model.count; ++i) {
            if (list.model.get(i).name === name) {
                list.currentIndex = i
                return list.currentItem
            }
        }
        return null
    }

    // The label in `row` that says `text` (its id here: no translations).
    function labelOf(row, text) {
        var found = findAll(row, function(x) { return x.font !== undefined && x.text === text }, [])
        return found.length > 0 ? found[0] : null
    }

    function rightEdge(item, row) {
        return Math.round(item.mapToItem(row, item.width, 0).x)
    }

    function placeLine(row) {
        var found = findAll(row, function(x) { return x.iconShown !== undefined }, [])
        return found.length > 0 ? found[0] : null
    }

    // Where a status ends: the page margin from the right edge of the row.
    function statusEdge(row) {
        return Math.round(row.width - Theme.horizontalPageMargin)
    }

    function checkNoCount() {
        var numbers = findAll(rowOf("Documents"), function(x) { return x.font !== undefined && /^[0-9]+$/.test(String(x.text)) }, [])
        expect(numbers.length === 0, "a folder row shows its count: " + (numbers.length > 0 ? numbers[0].text : ""))
    }

    function checkHeights() {
        var folder = rowOf("Documents")
        var edge = statusEdge(folder)
        var servers = [["NAS", "lautta-browse-server-connected"], ["Office", "lautta-browse-attention-auth"], ["Old", ""]]
        for (var i = 0; i < servers.length; ++i) {
            var row = rowOf(servers[i][0])
            expect(row.height === folder.height,
                   servers[i][0] + " is " + row.height + " tall, a folder " + folder.height)
            if (servers[i][1] === "")
                continue
            var status = labelOf(row, servers[i][1])
            if (!status) {
                fail(servers[i][0] + " shows no status")
                continue
            }
            expect(rightEdge(status, row) === edge,
                   servers[i][0] + "'s status ends at " + rightEdge(status, row) + ", the margin is at " + edge)
            expect(status.font.pixelSize === Theme.fontSizeSmall, servers[i][0] + "'s status is in the small font")
            var name = labelOf(row, servers[i][0])
            expect(name && rightEdge(name, row) <= status.mapToItem(row, 0, 0).x,
                   servers[i][0] + "'s name ends before its status")
        }
    }

    function checkVolumes() {
        var mounted = rowOf("SD card")
        var stick = rowOf("STICK")
        expect(stick.height === mounted.height,
               "STICK is " + stick.height + " tall, a mounted volume " + mounted.height)
        var size = placeLine(stick)
        expect(size && size.visible && size.text === Format.formatFileSize(8000000000),
               "STICK's line is " + (size ? size.text : "missing"))
        var status = labelOf(stick, "lautta-browse-not-mounted")
        if (!status || !status.visible)
            return fail("STICK does not say that it is not mounted")
        expect(rightEdge(status, stick) === statusEdge(stick),
               "STICK's status ends at " + rightEdge(status, stick) + ", the margin is at " + statusEdge(stick))
        expect(status.font.pixelSize === Theme.fontSizeSmall, "STICK's status is in the small font")
        var name = labelOf(stick, "STICK")
        expect(name && rightEdge(name, stick) <= status.mapToItem(stick, 0, 0).x, "STICK's name ends before its status")
        var other = labelOf(mounted, "lautta-browse-not-mounted")
        expect(!other || !other.visible, "a mounted volume says that it is not mounted")
    }

    function checkPlaces() {
        var remote = placeLine(rowOf("Photos"))
        var local = placeLine(rowOf("Thesis"))
        expect(remote && remote.text === "NAS › /srv/photos" && remote.iconShown,
               "a favourite on a server has the server icon before its place")
        var icons = findAll(remote, function(x) { return x.objectName === "remoteIcon" }, [])
        expect(icons.length === 1 && String(icons[0].source).indexOf("icon-m-computer") >= 0, "the icon is the server icon")
        expect(local && local.text === "Documents › Uni" && !local.iconShown, "a local favourite's place has no icon")
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            if (++win.waited > 100)
                return win.fail("the Browse page did not become active")
            if (!win.browse || win.browse.status !== PageStatus.Active || !win.rowOf("Old"))
                return
            win.checkNoCount()
            win.checkHeights()
            win.checkVolumes()
            win.checkPlaces()
            // Activity is reached from the pulley and the activity bar only.
            win.expect(!pageStack.nextPage(win.browse) && !win.browse._forwardDestination,
                       "Browse has a page attached: " + pageStack.nextPage(win.browse))
            win.scenarioRunning = false
        }
    }
}
