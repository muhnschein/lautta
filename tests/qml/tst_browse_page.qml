// SPDX-License-Identifier: LGPL-2.1-or-later
// Every kind of Browse row, so that all delegates are instantiated, and what
// they offer (SPEC §15.2): the pulley has no Connect to server and no
// Recents, servers' menus have Edit server and neither Disconnect nor
// Location settings, favourites' menus have Copy address and no Move up or
// down, a volume's has Open Storage settings only while the bridge offers it
// (NVB-13) and Unmount only when the bridge mounted it, a drive that is not
// mounted is dimmed, says so, has Mount and mounts when tapped instead of
// opening (LOC-3), and the Recents row of On this device opens Recents.
// Fails naming the row whose menu differs.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0

    // The visible entries of each row's menu, by name of the row. No
    // translations are loaded here: the texts are the ids.
    readonly property var menus: [
        { "name": "Thesis", "items": ["edit-favourite", "copy-address", "remove-favourite"] },
        { "name": "Documents", "items": ["add-favourite", "copy-address", "hide"] },
        { "name": "SD card", "items": ["add-favourite", "copy-address", "open-storage-settings", "hide"] },
        { "name": "CARD", "items": ["add-favourite", "copy-address", "unmount", "open-storage-settings", "hide"] },
        { "name": "STICK", "items": ["mount", "open-storage-settings", "hide"] },
        { "name": "NAS", "items": ["edit-server", "copy-address", "hide"] },
        { "name": "Office", "items": ["edit-server", "update-sign-in", "copy-address", "hide"] },
        { "name": "Vault", "items": ["edit-server", "review-identity", "copy-address", "hide"] },
        { "name": "host.example", "items": ["copy-address", "hide", "remove-recent"] },
        { "name": "files.kotisivu.fi", "items": ["connect-recent", "copy-address", "hide", "remove-recent"] }
    ]

    // Without the bridge's storage-settings the volume's menu lacks it.
    readonly property var withoutStorage: { "name": "SD card", "items": ["add-favourite", "copy-address", "hide"] }

    initialPage: Component {
        BrowsePage {
            storageSettings: true
            listModel: ListModel {
        ListElement { section: "favourites"; uri: "lautta://user-documents/Uni"; name: "Thesis"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; colour: "#e7a33c"; count: -1; itemId: "1"; place: "Documents › Uni"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "device"; uri: "lautta://user-documents/"; name: "Documents"; kind: "folder"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; count: 214; itemId: "user-documents"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "device"; uri: ""; name: ""; kind: "recents"; icon: "image://theme/icon-m-history"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "device"; uri: ""; name: ""; kind: "deleted"; icon: "image://theme/icon-m-delete"; status: "ready"; attention: ""; colour: ""; count: 8; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "android"; uri: "lautta://android-dcim/"; name: "DCIM"; kind: "folder"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; count: 3410; itemId: "android-dcim"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "volumes"; uri: "lautta://vol-SD/"; name: "SD card"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: "vol-SD"; place: ""; provider: ""; host: ""; free: 62500000000; total: 128000000000; fs: "exFAT" }
        ListElement { section: "volumes"; uri: "lautta://vol-CARD/"; name: "CARD"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: "vol-CARD"; place: ""; provider: ""; host: ""; free: 1000000000; total: 64000000000; fs: "vfat"; drive: "mmcblk1p1" }
        ListElement { section: "volumes"; uri: ""; name: "STICK"; kind: "volume"; icon: "image://theme/icon-m-usb"; status: "unmounted"; attention: ""; colour: ""; count: -1; itemId: "vol-STICK"; place: ""; provider: ""; host: ""; free: -1; total: 8000000000; fs: ""; drive: "sda1" }
        ListElement { section: "servers"; uri: ""; name: ""; kind: "consent"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: ""; name: ""; kind: "unavailable"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:1/"; name: "NAS"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; connection: "connected"; connectionError: ""; colour: ""; count: -1; itemId: "nv-account:1"; place: ""; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:2/"; name: "Office"; kind: "account"; icon: "image://theme/icon-m-file-folder"; status: "attention"; attention: "auth-failed"; connection: "failed"; connectionError: "AuthFailed"; colour: ""; count: -1; itemId: "nv-account:2"; place: ""; provider: "SMB"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:4/"; name: "Vault"; kind: "account"; icon: "image://theme/icon-m-file-folder"; status: "attention"; attention: "server-identity-changed"; connection: "failed"; connectionError: "ServerIdentityChanged"; colour: ""; count: -1; itemId: "nv-account:4"; place: ""; provider: "SFTP"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:3/"; name: "Cabin"; kind: "account"; icon: "image://theme/icon-m-file-folder"; status: "connecting"; attention: ""; connection: "connecting"; connectionError: ""; colour: ""; count: -1; itemId: "nv-account:3"; place: ""; provider: "SMB"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-adhoc:1/"; name: "host.example"; kind: "adhoc"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; connection: "disconnected"; connectionError: ""; colour: ""; count: -1; itemId: "nv-adhoc:1"; place: ""; provider: "SFTP"; host: "host.example"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "sftp://files.kotisivu.fi/"; name: "files.kotisivu.fi"; kind: "adhocRecent"; icon: "image://theme/icon-m-file-folder"; status: "offline"; attention: ""; connection: "disconnected"; connectionError: ""; colour: ""; count: -1; itemId: "nv-adhoc:9"; place: "sftp://files.kotisivu.fi/"; provider: "SFTP"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "nearby"; uri: "sftp://raspberrypi.local/"; name: "raspberrypi"; kind: "nearby"; icon: "image://theme/icon-m-wlan"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: "0"; place: ""; provider: "SFTP"; host: "raspberrypi.local"; free: -1; total: -1; fs: "" }
            }
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
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

    function isMenuItem(x) {
        return x.__silica_menuitem !== undefined
    }

    // The Browse page's list, laid out (headless, no render loop polishes it).
    function browseList() {
        var views = findAll(pageStack.currentPage, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        if (views.length > 0)
            views[0].forceLayout()
        return views.length > 0 ? views[0] : null
    }

    // The delegate of the row named `name` (of kind `kind` when given).
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

    // A drive that is not mounted (LOC-3): dimmed, saying so, and a tap
    // mounts it instead of opening a page for a volume that is not there.
    function checkUnmountedRow(list) {
        var row = rowOf(list, "STICK")
        if (!row || !row.unmounted)
            return fail("STICK is not shown as not mounted")
        if (findAll(row, function(x) { return x.text === "lautta-browse-not-mounted" && x.visible }, []).length !== 1)
            return fail("STICK does not say that it is not mounted")
        var stickDimmed = dimmed(row)
        var mounted = rowOf(list, "CARD")
        if (mounted.unmounted || findAll(mounted, function(x) { return x.text === "lautta-browse-not-mounted" && x.visible }, []).length > 0)
            return fail("CARD is shown as not mounted")
        // Its icon and its labels, and nothing of the mounted one.
        if (stickDimmed < dimmed(mounted) + 2)
            return fail("STICK is not dimmed")
        row = rowOf(list, "STICK")
        var depth = pageStack.depth
        row.clicked(null)
        if (pageStack.depth !== depth)
            fail("tapping STICK opened a page")
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
                // RecentsPage is the page that knows whether recents are on.
                if (pageStack.currentPage.recentsOn !== undefined)
                    win.scenarioRunning = false
                return
            }
            var list = win.browseList()
            if (!list || !list.currentItem)
                return
            win.checkPulley(list)
            for (var i = 0; i < win.menus.length && win.scenarioRunning; ++i)
                win.checkMenu(list, win.menus[i])
            pageStack.currentPage.storageSettings = false
            if (win.scenarioRunning)
                win.checkMenu(list, win.withoutStorage)
            if (win.scenarioRunning)
                win.checkUnmountedRow(list)
            if (win.scenarioRunning)
                win.checkRecentsRow(list)
            win.step = 1
        }
    }
}
