// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/scan/ScanMath.js" as ScanMath

Item {
    property bool reachedEnd

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function near(a, b) {
        return Math.abs(a - b) < 0.001
    }

    function same(a, b) {
        return JSON.stringify(a) === JSON.stringify(b)
    }

    function checkPages() {
        check(ScanMath.isPage(ScanMath.whole), "whole picture is a page")
        check(ScanMath.isPage([0.1, 0.2, 0.9, 0.1, 0.8, 0.9, 0.2, 0.8]), "skewed sheet")
        check(!ScanMath.isPage([0, 0, 0, 1, 1, 1, 1, 0]), "counter-clockwise")
        check(!ScanMath.isPage([1, 0, 0, 0, 1, 1, 0, 1]), "crossed")
        check(!ScanMath.isPage([0, 0, 1, 0, 0.2, 0.2, 0, 1]), "dent")
        check(!ScanMath.isPage([0, 0, 1, 0, 1, 1]), "three corners")
        check(!ScanMath.isPage(null) && !ScanMath.isPage(undefined), "no corners")
        // As lautta-core: two percent wide and high is the least.
        check(ScanMath.isPage([0, 0, 0.021, 0, 0.021, 0.021, 0, 0.021]), "smallest page")
        check(!ScanMath.isPage([0, 0, 0.019, 0, 0.019, 0.019, 0, 0.019]), "too small")
        check(!ScanMath.isPage([0, 0, 1, 0, 1, 0, 0, 0]), "flat")
    }

    function checkMoves() {
        var start = [0.1, 0.1, 0.9, 0.1, 0.9, 0.9, 0.1, 0.9]
        check(same(ScanMath.moved(start, 0, 0.2, 0.3), [0.2, 0.3, 0.9, 0.1, 0.9, 0.9, 0.1, 0.9]), "top left moves")
        check(same(ScanMath.moved(start, 2, 0.7, 0.6), [0.1, 0.1, 0.9, 0.1, 0.7, 0.6, 0.1, 0.9]), "bottom right moves")
        check(same(ScanMath.moved(start, 3, 0.3, 0.8), [0.1, 0.1, 0.9, 0.1, 0.9, 0.9, 0.3, 0.8]), "bottom left moves")
        check(same(start, [0.1, 0.1, 0.9, 0.1, 0.9, 0.9, 0.1, 0.9]), "given corners stay as they were")
        check(same(ScanMath.moved(start, 1, 1.4, -0.3), [0.1, 0.1, 1, 0, 0.9, 0.9, 0.1, 0.9]), "kept inside the photo")
        check(same(ScanMath.moved(start, 0, -2, 5), start), "outside and no page: refused")
        check(ScanMath.moved(start, 0, 0.95, 0.95) === start, "past the opposite corner: refused")
        check(ScanMath.moved(start, 1, 0.1, 0.5) === start, "onto the left edge: refused")
    }

    function checkTurns() {
        var turns = [[0, 1, 1], [3, 1, 0], [0, -1, 3], [2, -1, 1], [1, 2, 3], [0, 0, 0], [3, -4, 3]]
        for (var i = 0; i < turns.length; ++i) {
            var got = ScanMath.turned(turns[i][0], turns[i][1])
            check(got === turns[i][2], turns[i][0] + " turned by " + turns[i][1] + ": " + got)
        }
    }

    function checkFit() {
        var upright = ScanMath.fit(400, 600, 1600, 1200, 0)
        check(near(upright.width, 400) && near(upright.height, 300), "wide photo in tall box: " + JSON.stringify(upright))
        // Turned: its 1200 side lies across the box, sizes stay those of the unturned photo.
        var turned = ScanMath.fit(400, 600, 1600, 1200, 1)
        check(near(turned.width, 533.333) && near(turned.height, 400), "turned once: " + JSON.stringify(turned))
        check(same(ScanMath.fit(400, 600, 1600, 1200, 2), upright), "turned twice as upright")
        check(same(ScanMath.fit(400, 600, 1600, 1200, 3), turned), "turned three times as once")
        var tall = ScanMath.fit(600, 300, 100, 200, 0)
        check(near(tall.width, 150) && near(tall.height, 300), "tall photo in wide box: " + JSON.stringify(tall))
        var none = { "width": 0, "height": 0 }
        check(same(ScanMath.fit(400, 600, 0, 0, 0), none), "no photo yet")
        check(same(ScanMath.fit(0, 600, 10, 10, 0), none) && same(ScanMath.fit(400, 0, 10, 10, 0), none), "no box")
        check(same(ScanMath.fit(400, 600, 10, 0, 0), none) && same(ScanMath.fit(400, 600, 0, 10, 1), none), "one side missing")
    }

    function checkEdges() {
        var c = [0.25, 0.5, 0.75, 0.5, 0.75, 1, 0.25, 1]
        var top = ScanMath.edge(c, 0, 400, 200)
        check(near(top.x, 100) && near(top.y, 100) && near(top.length, 200) && near(top.angle, 0), "top: " + JSON.stringify(top))
        var right = ScanMath.edge(c, 1, 400, 200)
        check(near(right.x, 300) && near(right.y, 100) && near(right.length, 100) && near(right.angle, 90),
              "right: " + JSON.stringify(right))
        var bottom = ScanMath.edge(c, 2, 400, 200)
        check(near(bottom.x, 300) && near(bottom.y, 200) && near(bottom.length, 200) && near(Math.abs(bottom.angle), 180),
              "bottom: " + JSON.stringify(bottom))
        // Last edge closes the outline back to the first corner.
        var left = ScanMath.edge(c, 3, 400, 200)
        check(near(left.x, 100) && near(left.y, 200) && near(left.length, 100) && near(left.angle, -90),
              "left: " + JSON.stringify(left))
        var slope = ScanMath.edge([0, 0, 0.75, 0.5, 1, 1, 0, 1], 0, 400, 800)
        check(near(slope.length, 500) && near(slope.angle, 53.130), "sloped: " + JSON.stringify(slope))
    }

    function checkCells() {
        var cells = [[0, 0, 0], [99, 129, 0], [100, 0, 1], [199, 129, 1], [0, 130, 2], [150, 200, 3],
                     [50, 260, 4], [150, 260, -1], [200, 50, -1], [-1, 50, -1], [50, -1, -1], [50, 400, -1]]
        for (var i = 0; i < cells.length; ++i) {
            var got = ScanMath.cellAt(cells[i][0], cells[i][1], 100, 130, 2, 5)
            check(got === cells[i][2], "cell at " + cells[i][0] + "," + cells[i][1] + ": " + got)
        }
        check(ScanMath.cellAt(250, 10, 100, 130, 3, 5) === 2, "third column")
        check(ScanMath.cellAt(10, 10, 0, 130, 2, 5) === -1 && ScanMath.cellAt(10, 10, 100, 0, 2, 5) === -1, "no cells yet")
        check(ScanMath.cellAt(10, 10, 100, 130, 2, 0) === -1, "no pages")
    }

    Component.onCompleted: {
        checkPages()
        checkMoves()
        checkTurns()
        checkFit()
        checkEdges()
        checkCells()
        reachedEnd = true
    }
}
