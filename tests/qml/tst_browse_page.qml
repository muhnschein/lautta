// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property int storageCalls: 0

    readonly property var menus: [
        { "name": "Thesis", "items": ["edit-favourite", "copy-address", "remove-favourite"] },
        { "name": "Documents", "items": ["add-favourite", "copy-address"] },
        { "name": "SD card", "items": ["add-favourite", "copy-address", "open-storage-settings"] },
        { "name": "CARD", "items": ["add-favourite", "copy-address", "unmount", "open-storage-settings"] },
        { "name": "STICK", "items": ["mount", "open-storage-settings"] },
        { "name": "VAULT", "items": ["open-storage-settings"] },
        { "name": "NAS", "items": ["edit-server", "copy-address"] },
        { "name": "Office", "items": ["edit-server", "update-sign-in", "copy-address"] },
        { "name": "Vault", "items": ["edit-server", "review-identity", "copy-address"] },
        { "name": "host.example", "items": ["copy-address", "remove-recent"] },
        { "name": "files.kotisivu.fi", "items": ["connect-recent", "copy-address", "remove-recent"] }
    ]

    readonly property var withoutStorage: { "name": "SD card", "items": ["add-favourite", "copy-address"] }

    initialPage: Component {
        BrowsePage {
            storageSettings: true
            listModel: ListModel {
        ListElement { section: "favourites"; uri: "lautta://user-documents/Uni"; name: "Thesis"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; colour: "#e7a33c"; itemId: "1"; place: "Documents › Uni"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "device"; uri: "lautta://user-documents/"; name: "Documents"; kind: "folder"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; itemId: "user-documents"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "device"; uri: ""; name: ""; kind: "recents"; icon: "image://theme/icon-m-history"; status: "ready"; attention: ""; colour: ""; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "device"; uri: ""; name: ""; kind: "deleted"; icon: "image://theme/icon-m-delete"; status: "ready"; attention: ""; colour: ""; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "android"; uri: "lautta://android-dcim/"; name: "DCIM"; kind: "folder"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; itemId: "android-dcim"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "volumes"; uri: "lautta://vol-SD/"; name: "SD card"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; colour: ""; itemId: "vol-SD"; place: ""; provider: ""; host: ""; free: 62500000000; total: 128000000000; fs: "exFAT" }
        ListElement { section: "volumes"; uri: "lautta://vol-CARD/"; name: "CARD"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; colour: ""; itemId: "vol-CARD"; place: ""; provider: ""; host: ""; free: 1000000000; total: 64000000000; fs: "vfat"; drive: "mmcblk1p1" }
        ListElement { section: "volumes"; uri: ""; name: "STICK"; kind: "volume"; icon: "image://theme/icon-m-usb"; status: "unmounted"; attention: ""; colour: ""; itemId: "vol-STICK"; place: ""; provider: ""; host: ""; free: -1; total: 8000000000; fs: ""; drive: "sda1" }
        ListElement { section: "volumes"; uri: ""; name: "VAULT"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "locked"; attention: ""; colour: ""; itemId: "vol-VAULT"; place: ""; provider: ""; host: ""; free: -1; total: 32000000000; fs: ""; drive: "mmcblk1p2" }
        ListElement { section: "servers"; uri: ""; name: ""; kind: "consent"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; colour: ""; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: ""; name: ""; kind: "unavailable"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:1/"; name: "NAS"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "connected"; connectionError: ""; colour: ""; itemId: "nv-account:1"; place: ""; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:2/"; name: "Office"; kind: "account"; icon: "image://theme/icon-m-file-folder"; status: "attention"; attention: "auth-failed"; connection: "failed"; connectionError: "AuthFailed"; colour: ""; itemId: "nv-account:2"; place: ""; provider: "SMB"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:4/"; name: "Vault"; kind: "account"; icon: "image://theme/icon-m-file-folder"; status: "attention"; attention: "server-identity-changed"; connection: "failed"; connectionError: "ServerIdentityChanged"; colour: ""; itemId: "nv-account:4"; place: ""; provider: "SFTP"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:3/"; name: "Cabin"; kind: "account"; icon: "image://theme/icon-m-file-folder"; status: "connecting"; attention: ""; connection: "connecting"; connectionError: ""; colour: ""; itemId: "nv-account:3"; place: ""; provider: "SMB"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-adhoc:1/"; name: "host.example"; kind: "adhoc"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; connection: "disconnected"; connectionError: ""; colour: ""; itemId: "nv-adhoc:1"; place: ""; provider: "SFTP"; host: "host.example"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "sftp://files.kotisivu.fi/"; name: "files.kotisivu.fi"; kind: "adhocRecent"; icon: "image://theme/icon-m-file-folder"; status: "offline"; attention: ""; connection: "disconnected"; connectionError: ""; colour: ""; itemId: "nv-adhoc:9"; place: "sftp://files.kotisivu.fi/"; provider: "SFTP"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "nearby"; uri: "sftp://raspberrypi.local/"; name: "raspberrypi"; kind: "nearby"; icon: "image://theme/icon-m-wlan"; status: "ready"; attention: ""; colour: ""; itemId: "0"; place: ""; provider: "SFTP"; host: "raspberrypi.local"; free: -1; total: -1; fs: "" }
            }
        }
    }

    Connections {
        target: Bridge
        onFailed: if (subject === "") win.storageCalls++
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
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

    function isMenuItem(x) {
        return x.__silica_menuitem !== undefined
    }

    // No render loop headless: lay out by hand.
    function browseList() {
        var views = findAll(pageStack.currentPage, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        if (views.length > 0)
            views[0].forceLayout()
        return views.length > 0 ? views[0] : null
    }

    function rowOf(list, name, kind) {
        var rows = list.model
        for (var i = 0; i < rows.count; ++i) {
            var r = rows.get(i)
            if (r.name === name && (kind === undefined || r.kind === kind)) {
                list.currentIndex = i
                return list.currentItem
            }
        }
        return null
    }

    function texts(items) {
        return items.map(function(x) { return String(x.text).replace("lautta-browse-", "") }).join(",")
    }

    function checkPulley(list) {
        var pulley = texts(findAll(list, isMenuItem, []))
        if (pulley !== "settings,add-server,transfers")
            fail("the pulley has " + pulley)
    }

    function checkMenu(list, spec) {
        var row = rowOf(list, spec.name)
        if (!row)
            return fail("no row " + spec.name)
        var menu = row.openMenu()
        var shown = texts(findAll(menu, function(x) { return isMenuItem(x) && x.visible }, []))
        if (shown !== spec.items.join(","))
            fail("the menu of " + spec.name + " has " + shown)
        row.closeMenu()
    }

    function dimmed(row) {
        return findAll(row, function(x) { return x.opacity === Theme.opacityHigh }, []).length
    }

    function statusIcons(row) {
        return findAll(row, function(x) { return x.visible && x.objectName === "statusDot" }, [])
            .map(function(dot) { return String(dot.color) }).join(",")
    }

    function checkUnmountedRow(list) {
        var row = rowOf(list, "STICK")
        if (!row || !row.unmounted)
            return fail("STICK is not shown as not mounted")
        if (statusIcons(row) !== String(Theme.primaryColor))
            return fail("STICK shows [" + statusIcons(row) + "], not a dot of the primary colour")
        var stickDimmed = dimmed(row)
        var mounted = rowOf(list, "CARD")
        if (mounted.unmounted || statusIcons(mounted) !== String(Theme.highlightColor))
            return fail("CARD is shown as not mounted: [" + statusIcons(mounted) + "]")
        if (stickDimmed < dimmed(mounted) + 2)
            return fail("STICK is not dimmed")
        row = rowOf(list, "STICK")
        var depth = pageStack.depth
        row.clicked(null)
        if (pageStack.depth !== depth)
            fail("tapping STICK opened a page")
        if (row.statusIndicator !== "busy" || Bridge.busyDrives.join(",") !== "sda1")
            fail("STICK, tapped, is " + row.statusIndicator + " with drives [" + Bridge.busyDrives + "] busy, not busy")
    }

    function checkLockedRow(list) {
        var row = rowOf(list, "VAULT")
        if (!row || !row.locked || row.unmounted)
            return fail("VAULT is not shown as locked")
        var locks = findAll(row, function(x) { return x.visible && x.objectName === "statusLock" }, [])
        if (locks.length !== 1 || statusIcons(row) !== "")
            return fail("VAULT shows " + locks.length + " padlocks and dots [" + statusIcons(row) + "]")
        if (dimmed(row) < dimmed(rowOf(list, "CARD")) + 2)
            return fail("VAULT is not dimmed")
        row = rowOf(list, "VAULT")
        var depth = pageStack.depth
        row.clicked(null)
        if (pageStack.depth !== depth || Bridge.busyDrives.length !== 0)
            fail("tapping VAULT opened a page or mounted it")
    }

    function checkLockedRowWithoutStorage(list) {
        var row = rowOf(list, "VAULT")
        if (row.menu !== null)
            return fail("VAULT has a menu without Open Storage settings")
        var depth = pageStack.depth
        row.clicked(null)
        if (pageStack.depth !== depth || Bridge.busyDrives.length !== 0)
            fail("tapping VAULT without storage settings opened a page or mounted it")
    }

    function checkRecentsRow(list) {
        var row = rowOf(list, "", "recents")
        var label = row ? findAll(row, function(x) { return x.text === "lautta-browse-recents" }, []) : []
        if (label.length !== 1)
            return fail("the Recents row is named " + (row ? texts(findAll(row, function(x) { return x.text !== undefined && x.text !== "" }, [])) : "nothing"))
        row.clicked(null)
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            if (++win.waited > 100)
                return win.fail("step " + win.step + " timed out")
            if (win.step === 1) {
                if (pageStack.currentPage.recentsOn === undefined || win.storageCalls === 0)
                    return
                if (win.storageCalls !== 1)
                    win.fail("Open Storage settings was asked " + win.storageCalls + " times")
                win.scenarioRunning = false
                return
            }
            var list = win.browseList()
            if (!list || !list.currentItem)
                return
            win.checkPulley(list)
            for (var i = 0; i < win.menus.length && win.scenarioRunning; ++i)
                win.checkMenu(list, win.menus[i])
            if (win.scenarioRunning)
                win.checkLockedRow(list)
            pageStack.currentPage.storageSettings = false
            if (win.scenarioRunning)
                win.checkMenu(list, win.withoutStorage)
            if (win.scenarioRunning)
                win.checkLockedRowWithoutStorage(list)
            if (win.scenarioRunning)
                win.checkUnmountedRow(list)
            if (win.scenarioRunning)
                win.checkRecentsRow(list)
            win.step = 1
        }
    }
}
