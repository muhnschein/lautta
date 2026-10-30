// SPDX-License-Identifier: LGPL-2.1-or-later Qt 5.6: moves in a tight loop share a millisecond; a
// move with the previous move's time scrolls nothing. Each move of a drag on a SilicaFlickable must
// scroll at once.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    ViewerTools { id: tools }

    SilicaFlickable {
        id: flick

        width: 200
        height: 300
        contentWidth: width
        contentHeight: 5000
        boundsBehavior: Flickable.StopAtBounds
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function drag(direction, moves) {
        var step = 10 * direction
        var from = direction > 0 ? 280 : 20
        var start = flick.contentY
        var last = start
        var lag = []
        tools.mouse(flick, "press", 100, from)
        for (var i = 1; i <= moves; ++i) {
            tools.mouse(flick, "move", 100, from - step * i)
            if (last !== start && Math.abs(flick.contentY - last - step) > 0.5)
                lag.push(i + ": " + (flick.contentY - last))
            last = flick.contentY
        }
        tools.mouse(flick, "release", 100, from - step * moves)
        flick.cancelFlick()
        expect(Math.abs(last - start) > 10 * (moves - 6), (direction > 0 ? "up" : "down") + ": the drag scrolls: " + (last - start))
        expect(lag.length === 0, (direction > 0 ? "up" : "down") + ": every move scrolls by its length: " + lag.join(", "))
    }

    Component.onCompleted: {
        drag(1, 24)
        drag(-1, 12)
    }
}
