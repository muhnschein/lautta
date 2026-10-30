// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/FileIcons.js" as FileIcons

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int waited: 0
    property var browse

    initialPage: Component {
        BrowsePage {
            id: browsePage

            Component.onCompleted: win.browse = browsePage
            listModel: ListModel {
                ListElement { section: "favourites"; uri: "lautta://nv-account:1/srv/photos"; name: "Photos"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "1"; place: "NAS › /srv/photos"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
                ListElement { section: "favourites"; uri: "lautta://user-documents/Uni"; name: "Thesis"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "2"; place: "Documents › Uni"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
                ListElement { section: "device"; uri: "lautta://user-documents/"; name: "Documents"; kind: "folder"; icon: "file-folder"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; count: 214; itemId: "user-documents"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
                ListElement { section: "volumes"; uri: "lautta://vol-SD/"; name: "SD card"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "vol-SD"; place: ""; provider: ""; host: ""; free: 62500000000; total: 128000000000; fs: "exFAT"; drive: "mmcblk1p1" }
                ListElement { section: "volumes"; uri: ""; name: "STICK"; kind: "volume"; icon: "image://theme/icon-m-usb"; status: "unmounted"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "vol-STICK"; place: ""; provider: ""; host: ""; free: -1; total: 8000000000; fs: ""; drive: "sda1" }
                ListElement { section: "volumes"; uri: "lautta://vol-CARD/"; name: "CARD"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "vol-CARD"; place: ""; provider: ""; host: ""; free: 1000; total: 2000; fs: "vfat"; drive: "" }
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
        if (pred(item) && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

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

    function icons(row) {
        return findAll(row, function(x) { return x.visible && x.objectName === "statusDot" }, [])
            .map(function(dot) { return String(dot.color) })
    }

    function statusSlot(row) {
        return findAll(row, function(x) { return x.indicator !== undefined && x.dotSize !== undefined }, [])[0]
    }

    function labelOf(row, text) {
        var found = findAll(row, function(x) { return x.font !== undefined && x.text === text && x.visible }, [])
        return found.length > 0 ? found[0] : null
    }

    function rightEdge(item, row) {
        return Math.round(item.mapToItem(row, item.width, 0).x)
    }

    function placeLine(row) {
        var found = findAll(row, function(x) { return x.iconShown !== undefined }, [])
        return found.length > 0 ? found[0] : null
    }

    function statusEdge(row) {
        return Math.round(row.width - Theme.horizontalPageMargin)
    }

    function texts(row) {
        return findAll(row, function(x) { return x.visible && x.font !== undefined && typeof x.text === "string" && x.text.length > 0 }, [])
            .map(function(label) { return label.text })
    }

    function checkNoCount() {
        var numbers = findAll(rowOf("Documents"), function(x) { return x.font !== undefined && /^[0-9]+$/.test(String(x.text)) }, [])
        expect(numbers.length === 0, "a folder row shows its count: " + (numbers.length > 0 ? numbers[0].text : ""))
    }

    function checkHeights() {
        var folder = rowOf("Documents")
        var edge = statusEdge(folder)
        var servers = [["NAS", String(Theme.highlightColor)], ["Office", String(statusSlot(rowOf("Office")).warningColor)], ["Old", ""]]
        for (var i = 0; i < servers.length; ++i) {
            var row = rowOf(servers[i][0])
            expect(row.height === folder.height,
                   servers[i][0] + " is " + row.height + " tall, a folder " + folder.height)
            expect(texts(row).join("|") === servers[i][0], servers[i][0] + " shows " + texts(row).join("|") + ", only its name")
            var shown = icons(row)
            expect(shown.join(",") === servers[i][1], servers[i][0] + " shows icons [" + shown.join(",") + "], not [" + servers[i][1] + "]")
            if (servers[i][1] === "")
                continue
            var slot = statusSlot(row)
            expect(rightEdge(slot, row) === edge,
                   servers[i][0] + "'s status ends at " + rightEdge(slot, row) + ", the margin is at " + edge)
            var name = labelOf(row, servers[i][0])
            expect(name && rightEdge(name, row) <= slot.mapToItem(row, 0, 0).x,
                   servers[i][0] + "'s name ends before its status")
        }
    }

    function checkVolumes() {
        var mounted = rowOf("SD card")
        var stick = rowOf("STICK")
        var card = rowOf("CARD")
        expect(stick.height === mounted.height,
               "STICK is " + stick.height + " tall, a mounted volume " + mounted.height)
        var size = placeLine(stick)
        expect(size && size.visible && size.text === Format.formatFileSize(8000000000),
               "STICK's line is " + (size ? size.text : "missing"))
        expect(texts(stick).join("|") === "STICK|" + Format.formatFileSize(8000000000),
               "STICK shows " + texts(stick).join("|") + ", only its name and size")
        expect(icons(stick).join(",") === String(Theme.primaryColor), "STICK shows [" + icons(stick).join(",") + "], a dot of the primary colour")
        var slot = statusSlot(stick)
        expect(slot && rightEdge(slot, stick) === statusEdge(stick),
               "STICK's status ends at " + (slot ? rightEdge(slot, stick) : "nowhere") + ", the margin is at " + statusEdge(stick))
        var name = labelOf(stick, "STICK")
        expect(name && slot && rightEdge(name, stick) <= slot.mapToItem(stick, 0, 0).x, "STICK's name ends before its status")
        expect(icons(mounted).join(",") === String(Theme.highlightColor), "a mounted drive shows [" + icons(mounted).join(",") + "], the dot of the highlight colour")
        expect(texts(mounted).length === 2, "a mounted volume shows " + texts(mounted).join("|") + ", only its name and line")
        expect(icons(card).length === 0, "a volume the bridge does not mount shows [" + icons(card).join(",") + "]")
    }

    function checkGlyphs() {
        var image = function (row) {
            return findAll(row, function(x) { return x.themed !== undefined && x.visible }, [])[0]
        }
        var folder = image(rowOf("Documents"))
        var want = String(Qt.resolvedUrl("../../qml/icons/" + FileIcons.sized("file-folder", Theme.iconSizeMedium) + ".svg"))
        expect(folder && String(folder.source) === want && folder.status === Image.Ready,
               "a folder row draws the app's folder glyph: " + (folder ? folder.source : "none"))
        var star = image(rowOf("Thesis"))
        expect(star && String(star.source) === "image://theme/icon-m-favorite", "a favourite's star is the theme's")
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
            win.checkGlyphs()
            win.expect(!pageStack.nextPage(win.browse) && !win.browse._forwardDestination,
                       "Browse has a page attached: " + pageStack.nextPage(win.browse))
            win.scenarioRunning = false
        }
    }
}
