// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the rows of the grid (GridRows.js, SPEC §15.3, BRW-4): 3
// tiles per row in portrait, more in landscape at the same tile size, the
// rows a folder takes, which item is under a point and the height of a row,
// which is its tallest cell's. Fails when the number per row, the grouping
// into rows, the item under a finger or a row's height is wrong.
import QtQuick 2.6
import "../../../qml/components/directory/GridRows.js" as GridRows

Item {
    id: root

    // A phone's screen, 540 by 960: 3 tiles of 180 in portrait.
    readonly property real tile: 180

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    // The items of each row of `count` items in a view `width` wide, as the
    // grid groups them.
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

    // Rotating regroups the same items into longer rows and back.
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

    Component.onCompleted: {
        testColumns()
        testRows()
        testIndexAt()
        testRegroup()
        testTallest()
    }
}
