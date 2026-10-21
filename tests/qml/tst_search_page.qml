// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): the search page (SRC-2, design:
// Search) has no pulley; its field and filter summary sit above the results
// list, not in its header. Every search resets the list, which then makes its
// first new row current and focuses it inside itself: a field in the list
// loses the focus. Fails when the field is in the list or loses its text or
// focus, when the summary does not open the filters, when typing after the
// filters does not search the typed name, when a search by type alone is
// cleared on the way back, or when rows are not under the heading, in
// portrait and in landscape.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property var page
    property var heading
    property var field
    property var list
    property var filters
    property var dialog
    property var searchModel
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

    // Headless, no render loop polishes the list: it takes in new rows (and
    // makes one current) only when laid out by hand.
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
        // The check's screen is 240x320: the heading fills most of it.
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
        expect(within(field, heading) && within(filters, heading), "field and filter summary are in the heading")
        expect(page.allowedOrientations === Orientation.All, "the page turns to landscape")
    }

    // The filters dialog is up: keeps it and the page's model.
    function filtersShown() {
        if (pageStack.busy || pageStack.currentPage === page)
            return false
        dialog = pageStack.currentPage
        if (dialog.typeIds === undefined || dialog.searchModel === undefined) {
            fail("the filter summary opens the filters")
            return false
        }
        searchModel = dialog.searchModel
        return true
    }

    function backOnPage() {
        return !pageStack.busy && pageStack.currentPage === page
    }

    function nameField() {
        return findAll(dialog, function (x) {
            return x.echoMode !== undefined && x.label !== undefined && x.placeholderText !== undefined
        }, [])[0]
    }

    function typeBox() {
        return findAll(dialog, function (x) {
            return x.currentIndex !== undefined && (x.label === "lautta-filters-type" || x.label === "Type")
        }, [])[0]
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
                win.filters = win.named("searchFilters")
                if (!win.heading || !win.field || !win.list || !win.filters)
                    return win.fail("heading, field, list and filter summary are found")
                win.checkStructure()
                win.field.forceActiveFocus()
                win.fieldY = win.field.mapToItem(win.page, 0, 0).y
                win.field.text = "thesis"
                win.next()
                break
            case 1:
                // 12 + 3 hits in two folders.
                if (win.rowCount() !== 15)
                    break
                win.checkFieldKept("thesis", "first search")
                win.checkLayout("portrait")
                win.field.text = "thesis_1"
                win.next()
                break
            case 2:
                // The model was reset for the new name: thesis_1, thesis_10, thesis_11.
                if (win.rowCount() !== 3)
                    break
                win.checkFieldKept("thesis_1", "second search")
                win.filters.clicked(null)
                win.next()
                break
            case 3:
                if (!win.filtersShown())
                    break
                var name = win.nameField()
                win.expect(name.text === "thesis_1", "the filters start from the typed name: " + name.text)
                name.text = "final"
                win.dialog.accept()
                win.next()
                break
            case 4:
                if (!win.backOnPage() || win.rowCount() !== 1)
                    break
                win.expect(win.field.text === "final", "the field shows the name the filters searched for: " + win.field.text)
                win.field.forceActiveFocus()
                win.field.text = "thesis"
                win.next()
                break
            case 5:
                // Typing after the filters searches the typed name, not the dialog's.
                if (win.rowCount() !== 15)
                    break
                win.expect(win.searchModel.query === "thesis", "the query follows the field: " + win.searchModel.query)
                win.checkFieldKept("thesis", "after the filters")
                win.filters.clicked(null)
                win.next()
                break
            case 6:
                if (!win.filtersShown())
                    break
                // Folders of any name: the emptied field must not clear them.
                win.nameField().text = ""
                win.typeBox().currentIndex = 1
                win.dialog.accept()
                win.next()
                break
            case 7:
                // Past the typing delay, the two folders are still listed.
                if (win.waited < 8 || !win.backOnPage() || win.rowCount() !== 2)
                    break
                win.expect(win.field.text === "", "the field shows the empty name: " + win.field.text)
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
                win.scenarioRunning = false
                break
            }
        }
    }
}
