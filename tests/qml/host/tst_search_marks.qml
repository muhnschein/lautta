// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/search/Marks.js" as Marks

QtObject {
    property bool reachedEnd
    function same(got, want, what) {
        if (JSON.stringify(got) !== JSON.stringify(want))
            console.error("FAILED: " + what + ": " + JSON.stringify(got) + ", not " + JSON.stringify(want))
    }

    Component.onCompleted: {
        var on = "<b><font color=\"#0f0\">"
        var off = "</font></b>"
        same(Marks.marked("thesis_final.pdf", "thesis", "#0f0"), on + "thesis" + off + "_final.pdf",
             "the match in bold highlight colour")
        same(Marks.marked("Thesis notes thesis", "THESIS", "#0f0"),
             on + "Thesis" + off + " notes " + on + "thesis" + off, "every match, any case, keeps the name's case")
        same(Marks.marked("a<b>&c.txt", "b", "#0f0"), "a&lt;" + on + "b" + off + "&gt;&amp;c.txt",
             "the rest of the name is escaped")
        same(Marks.marked("notes.md", "", "#0f0"), "", "no needle, nothing marked")
        same(Marks.marked("notes.md", "xyz", "#0f0"), "", "no match, nothing marked")
        same(Marks.marked("", "a", "#0f0"), "", "no name, nothing marked")
        same(Marks.marked("İstanbul", "stan", "#0f0"), "", "a name whose case folding changes its length is left plain")
        same(Marks.needle("budget_2026.ods", 0, 6), "budget", "the needle is the matched part of the name")
        same(Marks.needle("budget_2026.ods", 3, 0), "", "no match length, no needle")
        same(Marks.escaped("<&>"), "&lt;&amp;&gt;", "escaping")
        reachedEnd = true
    }
}
