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
    property int pageWidth
    property real shownWidth
    property var page
    readonly property var expected: [
        ["lautta-date-today", "Today"],
        ["lautta-date-yesterday", "Yesterday"],
        ["lautta-date-last-7-days", "Last 7 days"],
        ["lautta-date-last-30-days", "Last 30 days"],
        ["lautta-date-older", "Older"]
    ]

    ListModel {
        id: rows

        ListElement { itemId: 1; uri: "lautta://nv-account:1/srv/photos/IMG_2041.jpg"; name: "IMG_2041.jpg"; kind: "opened"; place: "NAS › /srv/photos"; at: 1; dateGroup: "today"; category: "image"; mimeType: "image/jpeg"; thumbnailSource: "image://lautta-thumb/lautta://nv-account:1/srv/photos/IMG_2041.jpg"; textThumb: false; textMono: false; typeIcon: "types/text" }
        ListElement { itemId: 2; uri: "lautta://user-documents/notes.txt"; name: "notes.txt"; kind: "previewed"; place: "Documents"; at: 2; dateGroup: "today"; category: "text"; mimeType: "text/plain"; thumbnailSource: ""; textThumb: true; textMono: false; typeIcon: "types/text" }
        ListElement { itemId: 7; uri: "lautta://user-documents/a.txt"; name: "a.txt"; kind: "opened"; place: "Documents"; at: 7; dateGroup: "today"; category: "text"; mimeType: "text/plain"; thumbnailSource: ""; textThumb: false; textMono: false; typeIcon: "types/text" }
        ListElement { itemId: 8; uri: "lautta://user-documents/b.txt"; name: "b.txt"; kind: "opened"; place: "Documents"; at: 8; dateGroup: "today"; category: "text"; mimeType: "text/plain"; thumbnailSource: ""; textThumb: false; textMono: false; typeIcon: "types/text" }
        ListElement { itemId: 3; uri: "lautta://nv-account:1/invoice-0912.pdf"; name: "invoice-0912.pdf"; kind: "transferred"; place: "NAS"; at: 3; dateGroup: "yesterday"; category: "pdf"; mimeType: "application/pdf"; thumbnailSource: ""; textThumb: false; textMono: false; typeIcon: "types/text" }
        ListElement { itemId: 4; uri: "lautta://user-documents/README.md"; name: "README.md"; kind: "previewed"; place: "Documents"; at: 4; dateGroup: "last7Days"; category: "markdown"; mimeType: "text/markdown"; thumbnailSource: ""; textThumb: false; textMono: false; typeIcon: "types/text" }
        ListElement { itemId: 5; uri: "lautta://user-documents/budget.csv"; name: "budget.csv"; kind: "opened"; place: "Documents"; at: 5; dateGroup: "last30Days"; category: "text"; mimeType: "text/csv"; thumbnailSource: ""; textThumb: false; textMono: false; typeIcon: "types/text" }
        ListElement { itemId: 6; uri: "lautta://user-documents/old.csv"; name: "old.csv"; kind: "opened"; place: ""; at: 6; dateGroup: "older"; category: "text"; mimeType: "text/csv"; thumbnailSource: ""; textThumb: false; textMono: false; typeIcon: "types/text" }
        ListElement { itemId: 9; uri: "lautta://user-documents/letter.pdf"; name: "letter.pdf"; kind: "opened"; place: "Documents"; at: 9; dateGroup: "older"; category: "pdf"; mimeType: "application/pdf"; thumbnailSource: "file:///nowhere/letter.pdf"; textThumb: false; textMono: false; typeIcon: "types/text" }
    }

    ViewerTools { id: tools }

    initialPage: Component {
        Loader {
            source: Qt.resolvedUrl("../../qml/pages/RecentsPage.qml")
            onLoaded: {
                item.listModel = rows
                win.page = item
            }
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
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

    function texts(id, english) {
        return findAll(win.page, function(x) { return x.text === id || x.text === english })
    }

    function sections() {
        var views = findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })
        for (var i = 0; i < views.length; ++i) {
            views[i].cacheBuffer = 100000
            views[i].contentY += 1
            views[i].contentY -= 1
            views[i].forceLayout()
        }
        var found = findAll(win.page, function(x) { return x.objectName === "dateSection" })
        found.sort(function(a, b) { return a.y - b.y })
        return found.map(function(x) { return x.text })
    }

    function inside(ancestor, item) {
        for (var p = item; p; p = p.parent) {
            if (p === ancestor)
                return true
        }
        return false
    }

    function checkHeaderPinned() {
        var host = findAll(win.page, function(x) { return x.objectName === "recentsPulleyHost" })[0]
        var header = findAll(win.page, function(x) { return x.objectName === "recentsHeader" })[0]
        var view = findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })[0]
        var menus = findAll(win.page, function(x) { return x._isPullDownMenu === true })
        if (!host || !header || !view)
            return fail("the page has a pulley host, a header and a list")
        if (view.headerItem !== null || inside(view.contentItem, header))
            return fail("the header is not part of the list")
        if (menus.length !== 1 || menus[0].flickable !== host || view.pullDownMenu)
            return fail("the one pulley is the host's, not the list's")
        if (view.boundsBehavior !== Flickable.StopAtBounds)
            return fail("the list stops at its top, so a pull goes on to the host")
        if (host.contentHeight !== host.height || header.mapToItem(win.page, 0, 0).y !== 0)
            return fail("the host does not scroll and the header is at the top of the page")
        if (Math.abs(view.mapToItem(win.page, 0, 0).y - header.height) > 0.5 || !view.clip)
            return fail("the list starts under the header and is cut there")
        if (view.quickScroll)
            return fail("the date pill is the quick way through the list, not Silica's quick scroll arrows")
        view.contentY = view.originY + view.height
        var stays = header.mapToItem(win.page, 0, 0).y === 0 && host.contentY === 0
        view.contentY = view.originY
        if (!stays)
            return fail("the header stays when the list scrolls")
        return true
    }

    function checkWords(bar) {
        var now = Date.now()
        var today = new Date()
        var at = function (daysBack) {
            return new Date(today.getFullYear(), today.getMonth(), today.getDate() - daysBack, 12).getTime()
        }
        var expectWords = function (row, text, english, description, what) {
            var got = bar.words(row, now)
            if ((got.text !== text && got.text !== english) || got.description !== description)
                return fail(what + ": " + JSON.stringify(got))
            return true
        }
        var day = function (ms) { return Format.formatDate(new Date(ms), Formatter.DateMediumWithoutYear) }
        var month = new Date(at(40))
        return expectWords({ "inProgress": true, "at": at(0) }, "lautta-date-in-progress", "In progress", "", "a running task")
            && expectWords({ "inProgress": false, "at": at(0) }, "lautta-date-today", "Today", day(at(0)), "today")
            && expectWords({ "inProgress": false, "at": at(1) }, "lautta-date-yesterday", "Yesterday", day(at(1)), "yesterday")
            && expectWords({ "inProgress": false, "at": at(4) }, Qt.locale().standaloneDayName(new Date(at(4)).getDay(), Locale.LongFormat),
                           "", day(at(4)), "four days back: the weekday")
            && expectWords({ "inProgress": false, "at": at(40) }, Format.formatDate(month, Format.MonthNameStandalone),
                           "", String(month.getFullYear()), "forty days back: month and year")
    }

    function checkTopRow(bar, view, index, what) {
        var item = null
        for (var y = view.originY; y < view.originY + view.contentHeight && !item; y += 2) {
            if (view.indexAt(view.width / 2, y) === index)
                item = y
        }
        if (item === null)
            return fail(what + ": row " + index + " is not in the list")
        view.contentY = item
        var got = bar.rowAtTop()
        view.contentY = view.originY
        if (got !== index)
            return fail(what + ": the row at the top is " + index + ", got " + got)
        return true
    }

    function checkDateBar(grid) {
        var bar = findAll(win.page, function(x) { return x.objectName === "scrollDateBar" })[0]
        var view = findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined })[0]
        if (!bar || bar.flickable !== view)
            return fail("the date bar follows the list")
        if (bar.visible || bar.active)
            return fail("the date bar hides while the list stands still")
        if (!checkWords(bar))
            return false
        var shown = grid ? 2 : 5
        var row = bar.rowDate(shown)
        var want = grid ? 3 : 4
        if (!row || row.at !== want || row.inProgress)
            return fail((grid ? "grid row " : "row ") + shown + " is dated by its recent, at " + want + ": " + JSON.stringify(row))
        return checkTopRow(bar, view, shown, grid ? "grid" : "list")
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

    function hint() {
        var placeholders = findAll(win.page, function(x) { return x.hintText !== undefined && x.text !== undefined })
        return placeholders.length === 1 ? placeholders[0].hintText : "(" + placeholders.length + " placeholders)"
    }

    function checkPage() {
        var got = sections()
        if (!matches(got))
            return waited > 100 ? fail("sections are the date groups, got " + JSON.stringify(got)) : false
        if (texts("lautta-recents-clear", "Clear recents").length !== 1)
            return fail("the pulley's entries are not where the test looks")
        if (texts("lautta-recents-turn-off", "Turn off recents").length + texts("lautta-recents-turn-on", "Turn on recents").length !== 0)
            return fail("the pulley does not switch recents off or on")
        var boxes = findAll(win.page, function(x) { return String(x).indexOf("ComboBox") === 0 })
        if (boxes.length !== 0)
            return fail("Recents have no Show filter")
        if (!checkPlaces())
            return false
        return checkIcons(findAll(win.page, function(x) { return x.objectName === "recentIcon" }), "list")
    }

    function checkIcons(icons, view) {
        if (icons.length !== rows.count)
            return fail(view + ": " + icons.length + " icons for " + rows.count + " rows")
        icons.sort(function(a, b) {
            var pa = a.mapToItem(win.page, 0, 0), pb = b.mapToItem(win.page, 0, 0)
            return pa.y - pb.y || pa.x - pb.x
        })
        for (var i = 0; i < icons.length; ++i) {
            var row = rows.get(i)
            if (icons[i].thumbnailSource !== row.thumbnailSource || icons[i].category !== row.category
                    || icons[i].mimeType !== row.mimeType)
                return fail(view + ": icon " + i + " is not " + row.name + "'s thumbnail, type and MIME type")
        }
        return true
    }

    function top(item) {
        return item.mapToItem(win.page, 0, 0).y
    }

    function checkGrid() {
        var got = sections()
        if (!matches(got))
            return waited > 100 ? fail("grid keeps the date groups, got " + JSON.stringify(got)) : false
        var headers = findAll(win.page, function(x) { return x.objectName === "dateSection" })
        headers.sort(function(a, b) { return top(a) - top(b) })
        var tiles = findAll(win.page, function(x) { return x.objectName === "recentGridTile" })
        if (tiles.length !== rows.count)
            return fail(tiles.length + " tiles for " + rows.count + " recents")
        var columns = win.page.columns
        for (var i = 0; i < tiles.length; ++i) {
            var name = tiles[i].itemName
            var row = null
            var inGroup = 0
            for (var r = 0; r < rows.count && !row; ++r) {
                inGroup = r > 0 && rows.get(r - 1).dateGroup === rows.get(r).dateGroup ? inGroup + 1 : 0
                if (rows.get(r).name === name)
                    row = { "index": inGroup, "group": rows.get(r).dateGroup }
            }
            var g = win.expected.map(function(e) { return e[0] }).indexOf("lautta-date-" + {
                "today": "today", "yesterday": "yesterday", "last7Days": "last-7-days",
                "last30Days": "last-30-days", "older": "older" }[row.group])
            var y = top(tiles[i])
            if (y < top(headers[g]) + headers[g].height || (g + 1 < headers.length && y >= top(headers[g + 1])))
                return fail(name + " is not under its date group's header")
            var column = row.index % columns
            if (Math.abs(tiles[i].mapToItem(win.page, 0, 0).x - column * win.page.cellWidth) > 1)
                return fail(name + " is not in column " + column)
        }
        var places = findAll(win.page, function(x) { return x.iconShown !== undefined })
        for (var p = 0; p < places.length; ++p) {
            if (places[p].text !== "" && places[p].horizontalAlignment !== Text.AlignHCenter)
                return fail("a tile's place is not centred under its name")
        }
        if (!checkPlaces())
            return false
        var lines = findAll(win.page, function(x) { return x.objectName === "textPreview" })
        if (lines.length !== 1 || lines[0].text !== "line one\nline two")
            return waited > 100 ? fail("only notes.txt's tile shows its first lines: "
                                       + JSON.stringify(lines.map(function(x) { return x.text }))) : false
        var icons = []
        for (var t = 0; t < tiles.length; ++t)
            icons = icons.concat(findAll(tiles[t], function(x) { return x.thumbnailReady !== undefined }))
        return checkIcons(icons, "grid")
    }

    function pdfThumbnail() {
        sections()
        var tiles = findAll(win.page, function(x) { return x.objectName === "recentGridTile" && x.itemName === "letter.pdf" })
        if (tiles.length !== 1)
            return null
        return findAll(tiles[0], function(x) { return x.priority !== undefined && x.mimeType === "application/pdf" })[0] || null
    }

    readonly property var places: ({
        "NAS › /srv/photos": true,
        "NAS": true,
        "Documents": false
    })

    function checkPlaces() {
        var used = findAll(win.page, function(x) {
            return typeof x.text === "string" && /lautta-recents-(opened|previewed|transferred)|Opened|Previewed|Transferred/.test(x.text)
        })
        if (used.length > 0)
            return fail("a line says how the file was used: " + used[0].text)
        var lines = findAll(win.page, function(x) { return x.iconShown !== undefined })
        if (lines.length !== rows.count)
            return fail(lines.length + " place lines for " + rows.count + " rows")
        for (var i = 0; i < lines.length; ++i) {
            var want = win.places[lines[i].text]
            if (lines[i].text !== "" && lines[i].iconShown !== want)
                return fail(lines[i].text + (want ? " has no server icon" : " has the server icon"))
        }
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
                if (win.page && win.checkPage() === true) {
                    if (win.checkHeaderPinned() !== true || win.checkDateBar(false) !== true)
                        return
                    if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/notes-crlf.txt"), "lautta://user-documents/notes.txt")
                            || !tools.installFixture(Qt.resolvedUrl("host/fixtures/short-lines.txt"), "lautta://user-documents/a.txt"))
                        return win.fail("fixture")
                    App.setSetting("view_mode", JSON.stringify("grid"))
                    win.step++
                    win.waited = 0
                }
                break
            case 1:
                if (win.checkGrid() === true) {
                    if (win.checkHeaderPinned() !== true || win.checkDateBar(true) !== true)
                        return
                    var drawn = win.pdfThumbnail()
                    if (!drawn)
                        return win.fail("letter.pdf's tile asks the system thumbnailer")
                    win.pageWidth = Math.round(drawn.sourceSize.width * 0.7)
                    drawn.implicitWidth = win.pageWidth
                    drawn.implicitHeight = drawn.sourceSize.width
                    win.shownWidth = drawn.parent.width
                    if (Math.abs(drawn.width - win.shownWidth) >= 1
                            || Math.abs(drawn.height - drawn.implicitHeight * win.shownWidth / win.pageWidth) >= 1)
                        return win.fail("letter.pdf's page, zoomed from the size Nemo tells, fills the tile across: "
                                        + drawn.width + "x" + drawn.height)
                    App.setSetting("view_mode", JSON.stringify("list"))
                    win.step++
                    win.waited = 0
                }
                break
            case 2:
                if (win.pdfThumbnail() === null) {
                    App.setSetting("view_mode", JSON.stringify("grid"))
                    win.step++
                    win.waited = 0
                }
                break
            case 3:
                var joined = win.pdfThumbnail()
                if (joined) {
                    if (joined.implicitWidth !== 0 || Math.abs(joined.width - win.shownWidth) >= 1)
                        return win.fail("letter.pdf's page, shown again, keeps the size seen: "
                                        + joined.implicitWidth + " " + joined.width)
                    App.setSetting("view_mode", JSON.stringify("list"))
                    App.setSetting("recents_enabled", JSON.stringify(false))
                    win.step++
                    win.waited = 0
                }
                break
            case 4:
                var h = win.hint()
                if (h === "lautta-recents-off-settings-hint" || h === "Turn them on in Settings") {
                    App.setSetting("recents_enabled", JSON.stringify(true))
                    win.reachedEnd = true
                    win.scenarioRunning = false
                } else if (win.waited > 50) {
                    win.fail("with recents off the hint points to Settings, got " + h)
                }
                break
            }
        }
    }
}
