// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/ScrollDate.js" as ScrollDate

QtObject {
    property bool reachedEnd

    function same(got, want, what) {
        if (got !== want)
            console.error("FAILED: " + what + ": " + got + ", not " + want)
    }

    function at(y, m, d, h, min) {
        return new Date(y, m - 1, d, h, min || 0).getTime()
    }

    Component.onCompleted: {
        var now = at(2026, 11, 25, 9, 30)
        same(ScrollDate.kind(now, now), "today", "now")
        same(ScrollDate.kind(at(2026, 11, 25, 0, 0), now), "today", "this midnight")
        same(ScrollDate.kind(at(2026, 11, 25, 23, 59), now), "today", "later today")
        same(ScrollDate.kind(at(2026, 12, 3, 12), now), "today", "later than today")
        same(ScrollDate.kind(at(2026, 11, 24, 23, 59), now), "yesterday", "a minute before midnight")
        same(ScrollDate.kind(at(2026, 11, 24, 0, 0), now), "yesterday", "yesterday's midnight")
        same(ScrollDate.kind(at(2026, 11, 23, 23, 59), now), "weekday", "two days back")
        same(ScrollDate.kind(at(2026, 11, 19, 0, 0), now), "weekday", "six days back")
        same(ScrollDate.kind(at(2026, 11, 18, 23, 59), now), "month", "seven days back")
        same(ScrollDate.kind(at(2025, 3, 1, 12), now), "month", "last year")
        same(ScrollDate.kind(at(2026, 11, 1, 0, 0), at(2026, 11, 1, 0, 30)), "today", "half an hour after midnight")
        same(ScrollDate.kind(at(2026, 10, 31, 23, 50), at(2026, 11, 1, 0, 10)), "yesterday", "twenty minutes, but yesterday")
        same(ScrollDate.daysAgo(at(2026, 3, 28, 12), at(2026, 3, 30, 1)), 2, "across a clock change in spring")
        same(ScrollDate.daysAgo(at(2026, 10, 24, 12), at(2026, 10, 26, 23)), 2, "across a clock change in autumn")
        same(ScrollDate.daysAgo(at(2025, 12, 31, 23), at(2026, 1, 1, 1)), 1, "across new year")
        reachedEnd = true
    }
}
