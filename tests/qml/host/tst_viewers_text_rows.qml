// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/viewers/TextRows.js" as TextRows

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function repeat(s, n) {
        return new Array(n + 1).join(s)
    }

    function shown(layout) {
        return layout.rows.map(function(r) { return r.number + ":" + r.text }).join("|")
    }

    function checkLines() {
        var two = TextRows.rows("first\nsecond")
        check(shown(two) === "1:first|2:second" && two.lines === 2, "two lines: " + shown(two) + ", lines " + two.lines)
        var blank = TextRows.rows("")
        check(shown(blank) === "1:" && blank.lines === 1, "an empty text is one empty line: " + shown(blank))
        check(TextRows.rows("a\n\nb").rows.length === 3, "an empty line is a row")
        check(TextRows.empty.rows.length === 0 && TextRows.empty.lines === 0, "no text, no rows")
    }

    function checkTabs() {
        var cases = [["\tx", "    x"], ["ab\tc", "ab  c"], ["abcd\te", "abcd    e"], ["a\t\tb", "a       b"],
                     ["abc\t", "abc "], ["no tabs", "no tabs"], ["", ""]]
        for (var i = 0; i < cases.length; ++i) {
            var got = TextRows.expandTabs(cases[i][0])
            check(got === cases[i][1], "tabs in '" + cases[i][0] + "': '" + got + "'")
        }
        var row = TextRows.rows("\tx = 1")
        check(shown(row) === "1:    x = 1", "rows expand tabs: " + shown(row))
    }

    function checkLongLines() {
        var exact = TextRows.rows(repeat("a", 1000))
        check(exact.rows.length === 1, "1000 characters are one row: " + exact.rows.length)
        var two = TextRows.rows(repeat("b", 2000))
        check(two.rows.length === 2 && two.rows[1].text.length === 1000, "2000 characters are two rows: " + two.rows.length)
        var longLine = TextRows.rows("x\n" + repeat("c", 2500) + "\ny")
        var numbers = longLine.rows.map(function(r) { return r.number }).join(",")
        var lengths = longLine.rows.map(function(r) { return r.text.length }).join(",")
        check(numbers === "1,2,0,0,3" && lengths === "1,1000,1000,500,1", "a long line goes on over rows: numbers "
              + numbers + ", lengths " + lengths)
        check(longLine.lines === 3, "lines " + longLine.lines)
        check(longLine.rows[1].text + longLine.rows[2].text + longLine.rows[3].text === repeat("c", 2500), "nothing is lost")

        var pair = "\uD83D\uDE00"
        var emoji = TextRows.rows(repeat("d", 999) + pair + "e")
        check(emoji.rows.length === 2 && emoji.rows[0].text.length === 999 && emoji.rows[1].text === pair + "e",
              "the pair is not cut: " + emoji.rows.map(function(r) { return r.text.length }).join(","))
    }

    Component.onCompleted: {
        checkLines()
        checkTabs()
        checkLongLines()
    }
}
