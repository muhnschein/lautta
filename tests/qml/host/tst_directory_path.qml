// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/directory/PathFold.js" as PathFold
import "../../../qml/components/directory/PathNav.js" as PathNav

Item {
    id: root
    property bool reachedEnd

    readonly property real sep: 20
    readonly property real house: 50

    readonly property string pictures: "lautta://user-pictures/"
    readonly property string uni: "lautta://user-pictures/Uni"
    readonly property string thesis: "lautta://user-pictures/Uni/Thesis"
    readonly property string plots: "lautta://user-pictures/Uni/Thesis/Plots"

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function folded(widths, room) {
        return PathFold.folded(widths, root.sep, room)
    }

    function homeFits(widths, room) {
        return PathFold.homeFits(widths, root.sep, root.house, room)
    }

    function testTailWidth() {
        same(PathFold.tailWidth([100, 80, 120], sep, 0), 360, "all shown: each folder and the separator after it")
        same(PathFold.tailWidth([100, 80, 120], sep, 1), 240, "one folded: the rest")
        same(PathFold.tailWidth([100, 80, 120], sep, 3), 0, "all folded: nothing")
        same(PathFold.tailWidth([], sep, 0), 0, "a place's top folder: nothing before its name")
    }

    function testHomeFits() {
        same(homeFits([100, 80, 120], 430), true, "house, its separator and every folder fit exactly: house shows")
        same(homeFits([100, 80, 120], 429), false, "one pixel short: no house")
        same(homeFits([], 70), true, "a place's top folder: house and its separator fit exactly")
        same(homeFits([], 69), false, "a place's top folder, one pixel short: no house")
        same(homeFits([100], 0), false, "no room: no house")
    }

    function testFolded() {
        same(folded([100, 80, 120], 360), 0, "all fit exactly: nothing folds")
        same(folded([100, 80, 120], 359), 1, "one pixel short: the farthest folder folds")
        same(folded([100, 80, 120], 240), 1, "the rest fits exactly")
        same(folded([100, 80, 120], 239), 2, "still too wide: the next one folds")
        same(folded([100, 80, 120], 139), 3, "the nearest alone does not fit: all fold")
        same(folded([100, 80, 120], 0), 3, "no room at all: all fold")
        same(folded([100, 80, 120, 90, 70], 280), 3, "a deep path keeps the nearest folders")
        same(folded([], 100), 0, "a place's top folder: nothing to fold")
    }

    function testOffsets() {
        same(PathFold.offsets([100, 80, 120], sep, 0), [0, 120, 220], "all shown: each folder starts after the one before and its separator")
        same(PathFold.offsets([100, 80, 120], sep, 1), [0, 0, 100], "one folded: the first shown starts at the front")
        same(PathFold.offsets([100, 80, 120], sep, 3), [0, 0, 0], "all folded: none takes room")
        same(PathFold.offsets([], sep, 0), [], "no folders above: none")
    }

    function testMenuRows() {
        same(PathFold.menuRows(3, 8), [2, 1, 0, -1], "folded folders nearest first, then the way home")
        same(PathFold.menuRows(1, 8), [0, -1], "one folded folder: its row, then home")
        same(PathFold.menuRows(0, 8), [-1], "nothing folded: only home")
        same(PathFold.menuRows(5, 3), [4, 3, -1], "more than fit below: the nearest ones, home keeps its row")
        same(PathFold.menuRows(3, 4), [2, 1, 0, -1], "exactly as many as fit: all")
        same(PathFold.menuRows(3, 1), [-1], "room for one row: home")
        same(PathFold.menuRows(3, 0), [], "no room below: no rows")
    }

    function testPlan() {
        var browse = []
        var atPictures = [pictures]
        var atUni = [pictures, uni]
        var atThesis = [pictures, uni, thesis]
        var atPlots = [pictures, uni, thesis, plots]
        same(PathNav.plan([atPlots, atThesis, atUni, atPictures, browse], uni), { "pop": 2 },
             "walked down: back to that folder's own page")
        same(PathNav.plan([atPlots, atThesis, atUni, atPictures, browse], thesis), { "pop": 1 },
             "the parent: one page back")
        same(PathNav.plan([atPlots, atThesis, atUni, atPictures, browse], pictures), { "pop": 3 },
             "the place's top folder: its page")
        same(PathNav.plan([atPlots, browse], uni), { "replace": 0 },
             "entered deep from a favourite: no page of that folder, it takes this page's place")
        same(PathNav.plan([atPlots, atThesis, browse], uni), { "replace": 1 },
             "pages inside that folder all go: it takes the lowest one's place")
        same(PathNav.plan([atPlots, browse, atUni, atPictures, browse], uni), { "replace": 0 },
             "a page of that folder behind another kind of page is left alone")
        same(PathNav.plan([atPlots, atThesis, browse, atThesis], uni), { "replace": 1 },
             "only the run of folder pages on top counts")
        same(PathNav.plan([atPlots, ["lautta://user-documents/", "lautta://user-documents/Uni"], browse], uni), { "replace": 0 },
             "a folder of another place below: stays")
        same(PathNav.plan([atPlots], uni), { "replace": 0 }, "the only page: replaced")
        same(PathNav.plan([browse, atPlots], uni), { "replace": 0 }, "top page shows no folder: it is the one replaced")
    }

    Component.onCompleted: {
        testTailWidth()
        testHomeFits()
        testFolded()
        testOffsets()
        testMenuRows()
        testPlan()
        reachedEnd = true
    }
}
