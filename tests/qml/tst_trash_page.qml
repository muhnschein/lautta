// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property var page
    readonly property var expected: [
        ["lautta-date-today", "Today"],
        ["lautta-date-yesterday", "Yesterday"],
        ["lautta-date-last-30-days", "Last 30 days"]
    ]
    readonly property var seed: [
        { "trashId": 1, "name": "a.txt", "dateGroup": "today", "trashedAt": 1000, "daysLeft": 30 },
        { "trashId": 2, "name": "b.txt", "dateGroup": "today", "trashedAt": 990, "daysLeft": 30 },
        { "trashId": 3, "name": "c.txt", "dateGroup": "today", "trashedAt": 980, "daysLeft": 30 },
        { "trashId": 4, "name": "d.txt", "dateGroup": "today", "trashedAt": 970, "daysLeft": 30 },
        { "trashId": 5, "name": "e.txt", "dateGroup": "yesterday", "trashedAt": 900, "daysLeft": 29 },
        { "trashId": 6, "name": "f.txt", "dateGroup": "last30Days", "trashedAt": 800, "daysLeft": 8 },
        { "trashId": 7, "name": "g.txt", "dateGroup": "last30Days", "trashedAt": 700, "daysLeft": 7 },
        { "trashId": 8, "name": "h.txt", "dateGroup": "last30Days", "trashedAt": 600, "daysLeft": 1 }
    ]

    ListModel {
        id: rows
    }

    initialPage: Component {
        Page { }
    }

    function fail(what) {
        App.setSetting("view_mode", JSON.stringify("list"))
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function next() {
        step++
        waited = 0
    }

    function fill() {
        for (var i = 0; i < seed.length; ++i) {
            var r = seed[i]
            rows.append({ "trashId": r.trashId, "name": r.name, "dateGroup": r.dateGroup, "trashedAt": r.trashedAt,
                          "daysLeft": r.daysLeft, "folderUri": "lautta://user-documents/", "folderName": "Documents",
                          "isDir": false, "mimeType": "text/plain", "thumbnailSource": "" })
        }
    }

    function findAll(item, pred, out) {
        if (out === undefined) {
            out = []
            out.seen = []
        }
        if (!item || out.seen.indexOf(item) >= 0)
            return out
        out.seen.push(item)
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    function one(name) {
        return findAll(win.page, function(x) { return x.objectName === name })[0]
    }

    function view() {
        return findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })[0]
    }

    function inside(ancestor, item) {
        for (var p = item; p; p = p.parent) {
            if (p === ancestor)
                return true
        }
        return false
    }

    function layout() {
        var v = view()
        v.cacheBuffer = 100000
        v.contentY += 1
        v.contentY -= 1
        v.forceLayout()
        return v
    }

    function sections() {
        layout()
        var found = findAll(win.page, function(x) { return x.objectName === "dateSection" && x.visible })
        found.sort(function(a, b) { return a.mapToItem(win.page, 0, 0).y - b.mapToItem(win.page, 0, 0).y })
        return found.map(function(x) { return x.text })
    }

    function matches(got) {
        if (got.length !== expected.length)
            return false
        for (var i = 0; i < got.length; ++i) {
            if (got[i] !== expected[i][0] && got[i] !== expected[i][1])
                return false
        }
        return true
    }

    function checkAtTop(what) {
        var v = layout()
        var note = one("trashNote")
        if (!note || v.headerItem !== note)
            return fail(what + ": the note heads the list")
        if (Math.abs(v.contentY - v.originY) > 0.5 || Math.abs(note.mapToItem(v, 0, 0).y) > 0.5)
            return fail(what + ": the list shows its top, contentY " + v.contentY + " origin " + v.originY
                        + " note at " + note.mapToItem(v, 0, 0).y)
        return true
    }

    function checkHeaderPinned() {
        var host = one("trashPulleyHost")
        var header = one("trashHeader")
        var v = view()
        var menus = findAll(win.page, function(x) { return x._isPullDownMenu === true })
        if (!host || !header || !v)
            return fail("the page has a pulley host, a header and a list")
        if (inside(v.contentItem, header))
            return fail("the title is not part of the list")
        if (menus.length !== 1 || menus[0].flickable !== host || v.pullDownMenu)
            return fail("the one pulley is the host's, not the list's")
        if (v.boundsBehavior !== Flickable.StopAtBounds)
            return fail("the list stops at its top")
        if (Math.abs(v.mapToItem(win.page, 0, 0).y - header.height) > 0.5 || !v.clip)
            return fail("the list starts under the title and is cut there")
        v.contentY = v.originY + v.height
        var stays = header.mapToItem(win.page, 0, 0).y === 0 && host.contentY === 0
        v.contentY = v.originY
        if (!stays)
            return fail("the title stays when the list scrolls")
        return true
    }

    function byPlace(items) {
        return items.sort(function(a, b) {
            var pa = a.mapToItem(win.page, 0, 0), pb = b.mapToItem(win.page, 0, 0)
            return Math.abs(pa.y - pb.y) > 0.5 ? pa.y - pb.y : pa.x - pb.x
        })
    }

    function checkColoured(what, items, colourOf, warn) {
        if (items.length !== seed.length)
            return fail(what + ": " + items.length + " for " + seed.length + " items")
        byPlace(items)
        for (var i = 0; i < items.length; ++i) {
            var soon = seed[i].daysLeft <= 7
            if (Qt.colorEqual(colourOf(items[i]), warn) !== soon)
                return fail(what + ": " + seed[i].name + " with " + seed[i].daysLeft + " days left"
                            + (soon ? " is not" : " is") + " in the warning colour")
        }
        return true
    }

    function checkSoon(what) {
        layout()
        var warn = Theme.presenceColor(Theme.PresenceAway)
        var named = function(name) { return findAll(win.page, function(x) { return x.objectName === name }) }
        var colour = function(x) { return x.color }
        if (checkColoured(what + ", days left", named("trashDaysLeft"), colour, warn) !== true)
            return false
        if (!page.gridMode)
            return checkColoured(what + ", glyph", named("trashGlyph"), colour, warn)
        var glyphs = []
        var tiles = named("trashGridTile")
        for (var t = 0; t < tiles.length; ++t)
            glyphs = glyphs.concat(findAll(tiles[t], function(x) { return x.glyphColor !== undefined }))
        return checkColoured(what + ", glyph", glyphs, function(x) { return x.glyphColor }, warn)
                && checkColoured(what + ", tile", named("trashTileBackground"), colour,
                                 Theme.rgba(warn, Theme.highlightBackgroundOpacity / 3))
    }

    function checkNote() {
        var v = layout()
        var headers = findAll(win.page, function(x) { return x.title !== undefined && x.description !== undefined && x._titleItem !== undefined })
        if (headers.length !== 1 || headers[0].description !== "")
            return fail("the title has no subtitle: " + (headers.length ? headers[0].description : "no header"))
        var notes = findAll(one("trashNote"), function(x) { return x.wrapMode !== undefined })
        if (notes.length !== 1 || (notes[0].text !== "lautta-trash-note-kept" && notes[0].text.indexOf("kept here for 30 days") < 0))
            return fail("the note says how long Trash keeps files: " + (notes.length ? notes[0].text : "no note"))
        var note = notes[0]
        var above = note.mapToItem(v, 0, 0).y
        var sections = findAll(win.page, function(x) { return x.objectName === "dateSection" && x.visible })
        sections.sort(function(a, b) { return a.mapToItem(v, 0, 0).y - b.mapToItem(v, 0, 0).y })
        var first = sections[0]
        var textTop = first.mapToItem(v, 0, 0).y + first.topPadding + (first.height - first.topPadding - first.contentHeight) / 2
        var below = textTop - (above + note.height)
        if (above <= Theme.paddingMedium || Math.abs(above - below) > 1)
            return fail("the note sits as far under the title as Today's header sits under the note: " + above + " vs " + below)
        if (one("scrollDateBar"))
            return fail("Trash has no scroll date pill")
        return true
    }

    function checkGrid() {
        var tiles = findAll(win.page, function(x) { return x.objectName === "trashGridTile" })
        var listRows = findAll(win.page, function(x) { return x.objectName === "trashRow" })
        if (tiles.length !== seed.length || listRows.length !== 0)
            return fail(tiles.length + " tiles and " + listRows.length + " list rows for " + seed.length + " items")
        var columns = page.columns
        var tops = tiles.map(function(t) { return t.mapToItem(win.page, 0, 0).y })
        var first = Math.min.apply(Math, tops)
        var todayTop = tops.filter(function(y) { return Math.abs(y - first) < 0.5 })
        if (todayTop.length !== Math.min(columns, 4))
            return fail("today's first grid row holds " + todayTop.length + " tiles")
        return true
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            win.waited++
            if (win.waited > 150)
                return win.fail("step " + win.step + " timed out")
            switch (win.step) {
            case 0:
                App.setSetting("view_mode", JSON.stringify("list"))
                win.page = pageStack.push(Qt.resolvedUrl("../../qml/pages/TrashPage.qml"), { "listModel": rows })
                win.next()
                break
            case 1:
                if (win.page.status !== PageStatus.Active || !win.view())
                    break
                win.fill()
                win.next()
                break
            case 2:
                if (!win.matches(win.sections()))
                    break
                if (win.checkAtTop("items arriving after the page opened") !== true || win.checkHeaderPinned() !== true
                        || win.checkSoon("list") !== true || win.checkNote() !== true)
                    return
                App.setSetting("view_mode", JSON.stringify("grid"))
                win.next()
                break
            case 3:
                if (!win.page.gridMode || !win.matches(win.sections()))
                    break
                if (win.checkGrid() !== true || win.checkAtTop("grid") !== true || win.checkHeaderPinned() !== true
                        || win.checkSoon("grid") !== true || win.checkNote() !== true)
                    return
                App.setSetting("view_mode", JSON.stringify("list"))
                win.reachedEnd = true
                win.scenarioRunning = false
                break
            }
        }
    }
}
