// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/directory/GridRows.js" as GridRows

Item {
    id: root
    property bool reachedEnd

    readonly property real tile: 180

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function grouped(count, width) {
        var columns = GridRows.columns(width, root.tile, 3)
        var rows = []
        for (var r = 0; r < GridRows.rowCount(count, columns); ++r) {
            var row = []
            for (var x = root.tile / 2; x < width; x += width / columns) {
                var index = GridRows.indexAt(r, x, width / columns, columns, count)
                if (index >= 0)
                    row.push(index)
            }
            rows.push(row)
        }
        return rows
    }

    function testColumns() {
        same(GridRows.columns(540, root.tile, 3), 3, "portrait: 3 per row")
        same(GridRows.columns(960, root.tile, 3), 5, "landscape: as many tiles of portrait's size as fit best")
        same(GridRows.columns(1080, root.tile, 3), 6, "a wider screen: more")
        same(GridRows.columns(1000, root.tile, 3), 6, "the nearest number, not the number that fits whole")
        same(GridRows.columns(300, root.tile, 3), 3, "never fewer than portrait's 3")
        same(GridRows.columns(540, 0, 3), 3, "no tile size yet: portrait's number")
    }

    function testRows() {
        same(GridRows.rowCount(0, 3), 0, "an empty folder has no rows")
        same(GridRows.rowCount(1, 3), 1, "one item, one row")
        same(GridRows.rowCount(3, 3), 1, "a full row")
        same(GridRows.rowCount(4, 3), 2, "one more starts a row")
        same(GridRows.rowCount(1000, 3), 334, "a large folder")
        same(GridRows.rowCount(1000, 5), 200, "in landscape")
        same(GridRows.rowCount(5, 0), 0, "no columns, no rows")
        same([GridRows.rowOf(0, 3), GridRows.rowOf(2, 3), GridRows.rowOf(3, 3), GridRows.rowOf(7, 5)], [0, 0, 1, 1],
             "the row of an item")
        same(GridRows.rowOf(4, 0), 0, "and with no columns")
    }

    function testSpan() {
        same(GridRows.span(0, 3, 7), { "from": 0, "to": 3 }, "the first row's cells")
        same(GridRows.span(1, 3, 7), { "from": 3, "to": 6 }, "the second row's")
        same(GridRows.span(2, 3, 7), { "from": 6, "to": 7 }, "the last row holds what is left")
        same(GridRows.span(-1, 3, 7), { "from": 0, "to": 0 }, "a row the view has removed holds no cells")
    }

    function testIndexAt() {
        same(GridRows.indexAt(0, 0, 180, 3, 7), 0, "the first cell")
        same(GridRows.indexAt(0, 179.5, 180, 3, 7), 0, "up to its right edge")
        same(GridRows.indexAt(1, 200, 180, 3, 7), 4, "the second cell of the second row")
        same(GridRows.indexAt(2, 10, 180, 3, 7), 6, "the last item")
        same(GridRows.indexAt(2, 200, 180, 3, 7), -1, "past the last item")
        same(GridRows.indexAt(0, 540, 180, 3, 7), -1, "beside the row")
        same(GridRows.indexAt(-1, 10, 180, 3, 7), -1, "no row there")
        same(GridRows.indexAt(1, -1, 180, 3, 7), -1, "left of the row")
        same(GridRows.indexAt(0, 10, 0, 3, 7), -1, "a view of no width")
    }

    function testRegroup() {
        same(grouped(7, 540), [[0, 1, 2], [3, 4, 5], [6]], "7 items in portrait")
        same(grouped(7, 960), [[0, 1, 2, 3, 4], [5, 6]], "and in landscape")
        same(grouped(7, 540), [[0, 1, 2], [3, 4, 5], [6]], "and in portrait again")
    }

    function testTallest() {
        same(GridRows.tallest([]), 0, "a row with no cells yet has no height")
        same(GridRows.tallest([{ "implicitHeight": 260 }, { "implicitHeight": 310 }, { "implicitHeight": 280 }]), 310,
             "a row is as tall as its tallest cell")
        same(GridRows.tallest([{ "implicitHeight": 0 }, { "implicitHeight": 260 }]), 260,
             "an item of no height (the Repeater) changes nothing")
    }

    function testSectionRows() {
        same(GridRows.sectionRows(["today", "today", "today", "today", "yesterday", "older", "older"], 3),
             [{ "section": "today", "first": 0, "count": 3 }, { "section": "today", "first": 3, "count": 1 },
              { "section": "yesterday", "first": 4, "count": 1 }, { "section": "older", "first": 5, "count": 2 }],
             "a section starts its own row, a full row goes on in the next")
        same(GridRows.sectionRows(["today", "today", "today", "today", "yesterday"], 5),
             [{ "section": "today", "first": 0, "count": 4 }, { "section": "yesterday", "first": 4, "count": 1 }],
             "in landscape")
        same(GridRows.sectionRows([], 3), [], "nothing, no rows")
        same(GridRows.sectionRows(["today"], 0), [], "no columns, no rows")
    }

    ListModel {
        id: fitted

        property int changed
        property int inserted
        property int removed
        property int reset

        onDataChanged: changed++
        onRowsInserted: inserted++
        onRowsRemoved: removed++
        onModelReset: reset++
    }

    function rowsOf(model) {
        var out = []
        for (var i = 0; i < model.count; ++i) {
            var r = model.get(i)
            out.push({ "section": r.section, "first": r.first, "count": r.count })
        }
        return out
    }

    function testFitRows() {
        var three = GridRows.sectionRows(["today", "today", "today", "today", "older"], 3)
        GridRows.fitRows(fitted, three)
        same(rowsOf(fitted), three, "rows come")
        fitted.changed = fitted.inserted = fitted.removed = fitted.reset = 0
        GridRows.fitRows(fitted, three)
        same([fitted.changed, fitted.inserted, fitted.removed, fitted.reset], [0, 0, 0, 0], "same rows: nothing moves")
        var after = GridRows.sectionRows(["today", "today", "today", "older"], 3)
        GridRows.fitRows(fitted, after)
        same(rowsOf(fitted), after, "an item gone")
        same([fitted.changed, fitted.inserted, fitted.removed, fitted.reset], [1, 0, 1, 0],
             "second row changes, last goes, first stays: view keeps its place")
        GridRows.fitRows(fitted, three)
        same(rowsOf(fitted), three, "and back")
        same(fitted.inserted, 1, "a row comes at the end")
    }

    Component.onCompleted: {
        testColumns()
        testRows()
        testSpan()
        testIndexAt()
        testRegroup()
        testTallest()
        testSectionRows()
        testFitRows()
        reachedEnd = true
    }
}
