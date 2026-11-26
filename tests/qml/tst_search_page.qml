// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property var page
    property var heading
    property var field
    property var list
    property var pills
    property real fieldY

    SearchTestSupport { id: support }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
    }

    function next() {
        step++
        waited = 0
    }

    function findAll(item, pred, out) {
        if (!item)
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

    function named(name) {
        var found = findAll(page, function (x) { return x.objectName === name }, [])
        return found.length > 0 ? found[0] : null
    }

    function within(item, ancestor) {
        for (var p = item; p; p = p.parent) {
            if (p === ancestor)
                return true
        }
        return false
    }

    function rowCount() {
        list.forceLayout()
        return list.count
    }

    function checkFieldKept(text, what) {
        expect(list.currentIndex === 0, what + ": the list made its first row current")
        expect(field.text === text, what + ": the field keeps its text: " + field.text)
        expect(field.focus, what + ": the field keeps the focus")
        expect(field.mapToItem(page, 0, 0).y === fieldY, what + ": the field stays put")
    }

    function checkLayout(what) {
        list.forceLayout()
        var bottom = heading.mapToItem(page, 0, heading.height).y
        expect(heading.width === page.width, what + ": the heading is as wide as the page")
        expect(list.width === page.width, what + ": the list is as wide as the page")
        expect(Math.abs(list.mapToItem(page, 0, 0).y - bottom) < 1, what + ": the list starts at the heading's bottom")
        var rows = findAll(list.contentItem, function (x) {
            return x.openMenu !== undefined && x.contentHeight !== undefined
        }, [])
        expect(rows.length > 0, what + ": result rows are made")
        for (var i = 0; i < rows.length; ++i)
            expect(rows[i].mapToItem(page, 0, 0).y >= bottom, what + ": a row lies under the heading")
    }

    function checkStructure() {
        var pulleys = findAll(page, function (x) { return x._isPullDownMenu === true }, []).filter(function (x, i, all) {
            return all.indexOf(x) === i
        })
        expect(pulleys.length === 1, "the page has one pulley menu: " + pulleys.length)
        expect(!list.pullDownMenu, "the pulley belongs to the page, not to the result list")
        expect(named("searchSettingsItem") === null, "the pulley has no search settings entry")
        expect(named("clearRecentSearches") !== null, "the pulley can clear recent searches")
        expect(pulleys.length === 1 && !pulleys[0].visible, "without recent searches the pulley is hidden")
        expect(!within(field, list), "the search field is not inside the list")
        expect(list.headerItem === null, "the list has no header")
        expect(within(field, heading) && within(pills, heading), "field and pills are in the heading")
        expect(page.allowedOrientations === Orientation.All, "the page turns to landscape")
    }

    function pill(name) {
        return findAll(pills, function (x) { return x.objectName === name }, [])[0]
    }

    function touch(name) {
        return findAll(pill(name), function (x) { return x.preventStealing !== undefined }, [])[0]
    }

    function choice(menuName, value) {
        var menu = named(menuName)
        if (!menu)
            return null
        return findAll(menu, function (x) { return x.objectName === "pillChoice" && x.value === value }, [])[0]
    }

    function pick(menuName, value) {
        var menu = named(menuName)
        var item = choice(menuName, value)
        if (!menu || !item || !menu._expanded)
            return false
        menu._activatedMenuItem(item)
        return true
    }

    function marksShown() {
        var names = findAll(list.contentItem, function (x) { return x.objectName === "searchResultName" }, [])
        return names.length > 0 && names.every(function (n) {
            return n.textFormat === Text.StyledText && n.text.indexOf("<b><font") >= 0
        })
    }

    Component.onCompleted: {
        support.prepare()
        for (var i = 0; i < 12; ++i)
            support.write("Documents/srch_page/Uni/thesis_" + i + ".odt", "t", 0)
        for (var j = 0; j < 3; ++j)
            support.write("Documents/srch_page/Notes/thesis_note" + j + ".md", "n", 0)
        support.write("Documents/srch_page/final.pdf", "f", 0)
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
    }

    Component {
        id: pageComponent
        SearchPage { rootUri: "lautta://user-documents/srch_page" }
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
                win.page = pageStack.currentPage
                win.heading = win.named("searchHeading")
                win.field = win.named("searchField")
                win.list = win.named("searchList")
                win.pills = win.named("searchPills")
                if (!win.heading || !win.field || !win.list || !win.pills)
                    return win.fail("heading, field, list and pills are found")
                win.checkStructure()
                win.expect(!win.named("searchPlaceholder").enabled, "an empty search page shows no notice")
                win.expect(win.field.focus, "the field has the focus as the page opens")
                win.expect(win.page.scope === "folder", "a folder's search starts in the folder: " + win.page.scope)
                win.field.forceActiveFocus()
                win.fieldY = win.field.mapToItem(win.page, 0, 0).y
                win.field.text = "thesis"
                win.next()
                break
            case 1:
                if (win.rowCount() !== 15)
                    break
                win.checkFieldKept("thesis", "first search")
                win.expect(!win.named("searchPlaceholder").enabled, "no notice over results")
                win.checkLayout("portrait")
                win.field.text = "thesis_1"
                win.next()
                break
            case 2:
                if (win.rowCount() !== 3)
                    break
                win.checkFieldKept("thesis_1", "second search")
                win.field.text = "thesis"
                win.next()
                break
            case 3:
                if (win.rowCount() !== 15)
                    break
                win.expect(win.marksShown(), "every result marks the match in bold highlight")
                var pulley = win.findAll(win.page, function (x) { return x._isPullDownMenu === true }, [])[0]
                win.expect(win.within(pulley, win.named("searchRecents")),
                           "the pulley sits on the recent searches, away from the field")
                win.touch("typePill").clicked(null)
                win.next()
                break
            case 4:
                if (!win.named("typeMenu") || !win.named("typeMenu")._expanded)
                    break
                win.expect(win.named("searchPillHost").height > win.pills.height, "the type menu opens under the pills")
                win.expect(win.pill("typePill").open, "the Type pill shows its menu is open")
                if (!win.pick("typeMenu", "text"))
                    return win.fail("the type menu offers Text")
                win.next()
                break
            case 5:
                if (win.rowCount() !== 3)
                    break
                win.checkFieldKept("thesis", "type menu")
                win.expect(win.pill("typePill").checked, "a chosen type lights the Type pill")
                win.touch("typePill").clicked(null)
                win.touch("datePill").pressAndHold(null)
                win.next()
                break
            case 6:
                if (!win.named("dateMenu") || !win.named("dateMenu")._expanded)
                    break
                var typeMenu = win.named("typeMenu")
                win.expect(!typeMenu || !typeMenu.active, "one menu at a time")
                win.checkLayout("menu open")
                if (!win.pick("dateMenu", "today"))
                    return win.fail("the date menu offers Today")
                win.next()
                break
            case 7:
                if (win.waited < 8 || win.rowCount() !== 3 || win.named("dateMenu") !== null)
                    break
                win.expect(win.pill("datePill").checked, "a chosen date lights its pill")
                win.touch("typePill").clicked(null)
                win.next()
                break
            case 8:
                if (!win.pick("typeMenu", ""))
                    break
                win.next()
                break
            case 9:
                if (win.rowCount() !== 15)
                    break
                win.expect(!win.pill("typePill").checked, "any type dims the Type pill")
                win.touch("scopePill").clicked(null)
                win.next()
                break
            case 10:
                if (!win.named("scopeMenu") || !win.named("scopeMenu")._expanded)
                    break
                if (!win.choice("scopeMenu", "folder"))
                    return win.fail("the scope menu names the folder too")
                if (!win.pick("scopeMenu", "device"))
                    return win.fail("the scope menu offers This device")
                win.next()
                break
            case 11:
                if (win.rowCount() < 15 || win.named("searchGroupName") === null)
                    break
                win.expect(win.page.scope === "device" && win.page.wide, "the device scope is wide")
                win.expect(win.named("searchGroupName").text.length > 0, "the device section is named")
                win.touch("typePill").clicked(null)
                win.next()
                break
            case 12:
                var open = win.named("typeMenu")
                if (!open || !open._expanded)
                    break
                win.touch("typePill").clicked(null)
                win.next()
                break
            case 13:
                if (win.named("typeMenu") !== null)
                    break
                var host = win.named("searchPulleyHost")
                win.expect(Math.abs(host.contentY - host.originY) < 1,
                           "closing a menu by its pill leaves the page at its top: " + host.contentY)
                win.expect(Math.abs(win.named("searchPillHost").height - win.pills.height) < 1,
                           "a closed menu leaves no room under the pills")
                win.page.datePreset = "any"
                win.field.text = ""
                win.next()
                break
            case 14:
                if (!win.named("searchRecents").visible)
                    break
                var shown = win.findAll(win.page, function (x) { return x._isPullDownMenu === true }, [])[0]
                win.expect(shown.visible, "with the field empty, remembered searches bring the pulley")
                win.field.text = "thesis"
                win.next()
                break
            case 15:
                if (win.rowCount() !== 15)
                    break
                win.field.forceActiveFocus()
                win.deviceOrientation = Orientation.Landscape
                win.next()
                break
            case 16:
                if (!win.page.isLandscape || win.page.orientationTransitionRunning)
                    break
                win.expect(win.page.width > win.page.height, "landscape page is wide: " + win.page.width + "x" + win.page.height)
                win.checkLayout("landscape")
                win.expect(win.field.focus, "the field keeps the focus in landscape")
                win.reachedEnd = true
                win.scenarioRunning = false
                break
            }
        }
    }
}
