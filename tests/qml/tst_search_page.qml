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
        expect(findAll(page, function (x) { return x._isPullDownMenu === true }, []).length === 0,
               "the page has no pulley menu")
        expect(findAll(page, function (x) { return x.pullDownMenu !== undefined && x.pullDownMenu !== null }, []).length === 0,
               "no flickable of the page has a pulley menu")
        expect(!within(field, list), "the search field is not inside the list")
        expect(list.headerItem === null, "the list has no header")
        expect(within(field, heading) && within(pills, heading), "field and pills are in the heading")
        expect(page.allowedOrientations === Orientation.All, "the page turns to landscape")
    }

    function pill(name, type) {
        return findAll(pills, function (x) {
            return x.objectName === name && (type === undefined || x.type === type)
        }, [])[0]
    }

    function choice(choices, value) {
        return findAll(choices, function (x) { return x.objectName === "pillChoice" && x.value === value }, [])[0]
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
                var text = win.pill("typePill", "text")
                if (!text)
                    return win.fail("a Text pill")
                text.clicked()
                win.expect(text.checked, "the Text pill lights up")
                win.next()
                break
            case 4:
                if (win.rowCount() !== 3)
                    break
                win.checkFieldKept("thesis", "type pill")
                win.pill("typePill", "text").clicked()
                win.pill("datePill").clicked()
                win.next()
                break
            case 5:
                if (win.rowCount() !== 15)
                    break
                var dates = win.named("dateChoices")
                win.expect(dates.shown && dates.height > 0, "the date pill opens its choices")
                win.checkLayout("choices open")
                win.choice(dates, "today").clicked(null)
                win.expect(!dates.shown, "a choice closes the choices")
                win.expect(win.pill("datePill").checked, "a chosen date lights its pill")
                win.next()
                break
            case 6:
                if (win.waited < 8 || win.rowCount() !== 15)
                    break
                win.pill("scopePill").clicked()
                var scopes = win.named("scopeChoices")
                win.expect(scopes.shown, "the scope pill opens its choices")
                var device = win.choice(scopes, "device")
                if (!device || !win.choice(scopes, "folder"))
                    return win.fail("scope choices name the folder and the device")
                device.clicked(null)
                win.next()
                break
            case 7:
                if (win.rowCount() < 15 || win.named("searchGroupName") === null)
                    break
                win.expect(win.page.scope === "device" && win.page.wide, "the device scope is wide")
                win.expect(win.named("searchGroupName").text.length > 0, "the device section is named")
                win.field.forceActiveFocus()
                win.deviceOrientation = Orientation.Landscape
                win.next()
                break
            case 8:
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
