// SPDX-License-Identifier: LGPL-2.1-or-later
// Target test: Settings (SPEC §18) with real Silica and the real Lautta
// types, laid out as Salama's: the page is a list of entries, each opening a
// page of settings, with pages of their own for Deleting, Transfers, Search,
// Recents and Storage. Every control writes the key it wrote before; the
// thumbnails are in View options; no setting is left without a control.
// There is no date format (dates follow the system) nor grid size, and no
// view option is repeated (BRW-4): the View options entry opens that page. Fails when an
// entry is missing or opens another page, a page lacks one of its settings
// or a control writes nothing, or a setting can no longer be reached.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: window

    // Each entry of Settings, the page it opens and the keys set there.
    readonly property var subPages: [
        { "entry": "deletingEntry", "page": "deletingSettingsPage",
          "keys": ["recently_deleted", "recently_deleted_retention_days", "remorse_seconds"] },
        { "entry": "transfersEntry", "page": "transfersSettingsPage",
          "keys": ["local_concurrency", "remote_lanes", "preserve_mtimes", "preserve_permissions",
                   "verify_checksums", "auto_resume", "history_retention_days"] },
        { "entry": "searchEntry", "page": "searchSettingsPage", "keys": ["search_remote_depth"] },
        { "entry": "recentsEntry", "page": "recentsSettingsPage", "keys": ["recents_enabled"] },
        { "entry": "storageEntry", "page": "storageSettingsPage", "keys": ["listing_cache_days"] }
    ]
    readonly property var thumbnailKeys: ["thumbnails_remote", "thumbnail_max_remote_mb", "thumbnail_cache_mb"]
    // Set on View options by its own controls, and by "Ask before large
    // operations" two at once.
    readonly property var viewKeys: ["sort_key", "sort_descending", "folders_first", "show_hidden", "view_mode",
                                     "thumbnails_local"]
    readonly property var largeOpKeys: ["large_op_items", "large_op_bytes"]
    // Every key a page of Settings sets (nothing lost).
    property var reached: []

    ViewerTools { id: tools }

    function expect(ok, what) {
        if (!ok)
            console.error(what)
        return ok
    }

    // Items are reached both as children and as a content item: each once.
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

    function find(item, name) {
        var found = findAll(item, function (x) { return x.objectName === name }, [])
        return found.length === 1 ? found[0] : null
    }

    function controls(page) {
        return findAll(page, function (x) { return typeof x.settingKey === "string" }, [])
    }

    function keysOf(page) {
        return controls(page).map(function (x) { return x.settingKey })
    }

    // The rows of a combo box's menu, in order.
    function menuItems(box) {
        var kids = box.menu._contentColumn.children
        var out = []
        for (var i = 0; i < kids.length; ++i) {
            if (typeof kids[i].text === "string")
                out.push(kids[i])
        }
        return out
    }

    // Changes a setting through its control as a finger would: a switch is
    // pressed, a combo box gets another entry, a slider goes to another end.
    // Gives back the value the setting should have now.
    function change(control, before) {
        if (control.values !== undefined) {
            var other = control.values.filter(function (v) { return v !== before })[0]
            control.menu._activatedMenuItem(menuItems(control)[control.values.indexOf(other)])
            return other
        }
        if (control.maximumValue !== undefined) {
            // On release Silica takes the value from where the handle is:
            // the finger left it at one end.
            var drag = control.drag
            var toMin = before === control.maximumValue
            drag.target.x = toMin ? drag.minimumX : drag.maximumX
            control.released(null)
            return toMin ? control.minimumValue : control.maximumValue
        }
        control.clicked(null)
        return !before
    }

    // Each control writes its own key, and only that one.
    function exercise(control) {
        var key = control.settingKey
        var before = App.setting(key)
        var others = JSON.parse(App.settingsJson)
        delete others[key]
        var wanted = change(control, before)
        var after = JSON.parse(App.settingsJson)
        expect(after[key] === wanted, key + " is written by its control: " + after[key] + ", not " + wanted)
        delete after[key]
        expect(JSON.stringify(after) === JSON.stringify(others), key + "'s control writes no other setting")
        App.setSetting(key, JSON.stringify(before))
    }

    // Opens the page of one entry of Settings.
    function open(settings, entry) {
        var row = find(settings, entry)
        if (!expect(row !== null, "Settings has the entry " + entry))
            return null
        row.clicked(null)
        tools.pump(1000)
        return pageStack.currentPage
    }

    function checkSubPage(settings, spec) {
        var page = open(settings, spec.entry)
        if (!page || !expect(page.objectName === spec.page, spec.entry + " opens " + spec.page + ", not "
                             + (page ? page.objectName : "nothing")))
            return
        var keys = keysOf(page)
        expect(JSON.stringify(keys) === JSON.stringify(spec.keys), spec.page + " has " + spec.keys + ", not " + keys)
        controls(page).forEach(exercise)
        reached = reached.concat(keys)
        pageStack.pop(settings, PageStackAction.Immediate)
        tools.pump(200)
    }

    // "Ask before large operations" sets two keys from one menu.
    function checkLargeOps(settings) {
        var page = open(settings, "transfersEntry")
        var box = find(page, "largeOpsBox")
        if (expect(box !== null, "Transfers asks before large operations")) {
            box.menu._activatedMenuItem(menuItems(box)[2])
            expect(App.setting("large_op_items") === 10000 && App.setting("large_op_bytes") === 10000000000,
                   "the largest threshold is stored: " + App.setting("large_op_items"))
            box.menu._activatedMenuItem(menuItems(box)[1])
            expect(App.setting("large_op_items") === 1000, "and the default again")
            reached = reached.concat(largeOpKeys)
        }
        pageStack.pop(settings, PageStackAction.Immediate)
        tools.pump(200)
    }

    // Opened from Settings, View options has no folder; it has the
    // thumbnails, those from servers among them.
    function checkViewOptions(settings) {
        var options = open(settings, "viewOptionsRow")
        if (!expect(options.objectName === "viewOptionsPage" && !options.model,
                    "the row opens the view options page for every folder: " + options.objectName))
            return
        var keys = keysOf(options)
        expect(JSON.stringify(keys) === JSON.stringify(thumbnailKeys),
               "View options has the thumbnails of Settings: " + keys)
        controls(options).forEach(exercise)
        reached = reached.concat(keys).concat(viewKeys)
        pageStack.pop(settings, PageStackAction.Immediate)
        tools.pump(200)
    }

    // The entries tell what is chosen on their pages, and follow changes.
    function checkValues(settings) {
        var names = ["viewOptionsRow", "searchEntry", "recentsEntry", "deletingEntry", "transfersEntry", "storageEntry"]
        names.forEach(function (name) {
            var row = find(settings, name)
            expect(row && row.value.length > 0, name + " tells what is chosen: " + (row ? row.value : ""))
        })
        var deleting = find(settings, "deletingEntry")
        var before = deleting.value
        App.setSetting("recently_deleted", JSON.stringify(false))
        expect(deleting.value !== before, "Deleting tells that Trash is off: " + deleting.value)
        App.setSetting("recently_deleted", JSON.stringify(true))
    }

    Component.onCompleted: {
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(300)
        var settings = pageStack.currentPage
        var keys = keysOf(settings)
        expect(keys.length === 0, "Settings is a list of entries, with no setting of its own: " + keys)
        expect(find(settings, "browsePageEntry") !== null && find(settings, "aboutEntry") !== null,
               "Browse page and About stay on Settings")
        checkValues(settings)
        subPages.forEach(function (spec) { checkSubPage(settings, spec) })
        checkLargeOps(settings)
        checkViewOptions(settings)
        var all = Object.keys(JSON.parse(App.settingsJson))
        var lost = all.filter(function (k) { return reached.indexOf(k) < 0 })
        expect(lost.length === 0, "every setting has its control: none for " + lost)
        expect(all.indexOf("date_format") < 0 && reached.indexOf("date_format") < 0, "there is no date format")
        expect(all.indexOf("grid_columns") < 0, "there is no grid size")
    }

    Component {
        id: pageComponent

        SettingsPage { }
    }
}
