// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/directory/Selection.js" as Selection
import "../../../qml/components/directory/ContextActions.js" as Actions

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function ids(list) {
        return list.map(function (a) { return a.id })
    }

    function testSelectionStartEnd() {
        same(Selection.start(0), { "selecting": true, "waiting": true }, "pulley Select waits for the first item")
        same(Selection.start(2), { "selecting": true, "waiting": false }, "started with items picked")
        var idle = { "selecting": false, "waiting": false }
        same(Selection.afterCount(idle, 1), { "selecting": true, "waiting": false }, "first item starts it")
        same(Selection.afterCount({ "selecting": true, "waiting": false }, 0), idle,
             "dropping the last item ends it")
        same(Selection.afterCount({ "selecting": true, "waiting": true }, 0), { "selecting": true, "waiting": true },
             "an empty start keeps waiting")
        var picked = Selection.afterCount(Selection.start(0), 1)
        same(Selection.afterCount(picked, 0), idle, "after the first pick, dropping it ends selection")
        same(Selection.afterCount(idle, 0), idle, "nothing selected stays idle")
        same(Selection.afterCount({ "selecting": false, "waiting": true }, 0), idle, "waiting needs selecting")
    }

    function testSelectionActions() {
        var ending = ["copy", "cut", "share", "compress", "rename", "favourite", "moveToNewFolder"]
        for (var i = 0; i < ending.length; ++i)
            check(Selection.endsSelection(ending[i]), ending[i] + " ends selection")
        check(!Selection.endsSelection("delete"), "delete ends it only after the remorse timer")
        check(!Selection.endsSelection("copy_to") && !Selection.endsSelection(""), "other ids keep it")
    }

    function testPanel() {
        same(ids(Selection.panelButtons(3, true)), ["share", "copy", "cut", "delete", "more", "done"],
             "the menu's order, then More actions, then leaving")
        same(ids(Selection.panelButtons(1, true)), ["share", "copy", "cut", "delete", "more", "done"],
             "one item: the same buttons, no push-up menu")
        same(ids(Selection.panelButtons(3, false)), ["share", "copy", "done"], "read-only folder")
        same(ids(Selection.panelButtons(1, false)), ["share", "copy", "more", "done"],
             "read-only folder, one item: Add to favourites stays")
        same(ids(Selection.panelButtons(0, true)), ["done"], "waiting for the first item")
        var buttons = Selection.panelButtons(1, true)
        same(buttons.map(function (b) { return b.icon }),
             ["image://theme/icon-m-share", "dir-copy", "dir-cut", "image://theme/icon-m-delete",
              "more", "image://theme/icon-m-dismiss"], "icons")
        check(buttons[4].text === Actions.text("more") && buttons[0].text === Actions.text("share")
              && buttons[5].text === qsTrId("lautta-dir-sel-done"), "spoken names")
        check(Selection.panelMore === undefined, "no push-up menu")
    }

    function testMoreRows() {
        var file = { "isDir": false, "writable": true }
        same(ids(Selection.moreRows(1, file)), ["rename", "favourite", "compress", "moveToNewFolder"],
             "one item: Rename, then its own More actions page")
        same(ids(Selection.moreRows(1, { "isDir": true, "writable": true })),
             ["rename", "favourite", "compress", "moveToNewFolder"], "one folder")
        same(ids(Selection.moreRows(1, { "isDir": false, "writable": false })), ["favourite"],
             "no Rename or Compress in a read-only folder")
        same(ids(Selection.moreRows(2, file)), ["compress", "moveToNewFolder"],
             "several items: only what works on several")
        same(ids(Selection.moreRows(5, { "isDir": false, "writable": false })), [],
             "several, read-only: nothing, the archive would land there")
        same(Selection.moreRows(0, file), [], "nothing selected")
        same(Selection.moreRows(1, file).map(function (r) { return r.icon }),
             ["image://theme/icon-m-edit", "image://theme/icon-m-favorite", "file-archive", "file-folder"], "icons")
        same(Selection.moreRows(2, file).map(function (r) { return r.icon }), ["file-archive", "file-folder"],
             "Compress shows the glyph archives show")
        check(Selection.moreRows(1, file)[0].text === Actions.text("rename"), "menu texts")
        check(Selection.moreRows(1, file)[2].text === Actions.text("compress"), "menu texts, second")
        check(Selection.moreRows(1, file)[3].text === Actions.text("moveToNewFolder"), "menu texts, third")
    }

    function testPanelParity() {
        var contexts = [{ "isDir": false, "writable": true }, { "isDir": true, "writable": true },
                        { "isDir": false, "writable": false }, { "isDir": true, "writable": false }]
        for (var c = 0; c < contexts.length; ++c) {
            var ctx = contexts[c]
            var offered = ids(Selection.panelButtons(1, ctx.writable)).concat(ids(Selection.moreRows(1, ctx)))
            var menu = ids(Actions.menu(ctx)).concat(ids(Actions.more(ctx)))
            for (var i = 0; i < menu.length; ++i)
                check(menu[i] === "more" || offered.indexOf(menu[i]) >= 0,
                      menu[i] + " of the long-press menu is on the panel or its page for " + JSON.stringify(ctx))
            same(ids(Selection.moreRows(1, ctx)).filter(function (id) { return id !== "rename" }), ids(Actions.more(ctx)),
                 "the page holds the item's own More actions rows: " + JSON.stringify(ctx))
        }
    }

    function dragOver(states, path) {
        var now = states.slice()
        var before = function (i) { return states[i] }
        var drag = Selection.dragStart(path[0], states[path[0]])
        for (var p = 0; p < path.length; ++p) {
            var step = Selection.dragTo(drag, path[p], before)
            drag = step.drag
            for (var f = 0; f < step.flips.length; ++f)
                now[step.flips[f]] = !now[step.flips[f]]
        }
        return now
    }

    function testDragRange() {
        var none = [false, false, false, false, false, false, false, false]
        same(dragOver(none, [3]), [false, false, false, true, false, false, false, false], "holding selects the item")
        same(dragOver(none, [3, 4, 6]), [false, false, false, true, true, true, true, false],
             "the range follows the finger, reading order")
        same(dragOver(none, [3, 6, 4]), [false, false, false, true, true, false, false, false],
             "moving back shrinks the range")
        same(dragOver(none, [3, 6, 1]), [false, true, true, true, false, false, false, false],
             "passing the held item turns the range round")
        same(dragOver(none, [3, 7, 3]), [false, false, false, true, false, false, false, false],
             "back on the held item: only it")
        var some = [true, false, true, false, false, true, false, false]
        same(dragOver(some, [3, 6]), [true, false, true, true, true, true, true, false],
             "a range adds to what was selected")
        same(dragOver(some, [3, 6, 3]), some.map(function (s, i) { return s || i === 3 }),
             "items the range leaves get their earlier state back")
        same(dragOver(some, [2, 0]), [false, false, false, false, false, true, false, false],
             "started on a selected item, the range deselects")
        same(dragOver(some, [2, 0, 1, 7, 2]), [true, false, false, false, false, true, false, false],
             "a deselecting range restores what it leaves")
    }

    function testDragSteps() {
        var before = function (i) { return i === 5 }
        var drag = Selection.dragStart(2, false)
        same(drag, { "anchor": 2, "target": true, "from": 2, "to": 1 }, "covers nothing at first")
        var step = Selection.dragTo(drag, 2, before)
        same(step.flips, [2], "the first step flips the held item")
        step = Selection.dragTo(step.drag, 6, before)
        same(step.flips, [3, 4, 6], "only items entering the range, without the one already selected")
        same(step.drag, { "anchor": 2, "target": true, "from": 2, "to": 6 }, "range after the step")
        step = Selection.dragTo(step.drag, 6, before)
        same(step.flips, [], "the same item again changes nothing")
        step = Selection.dragTo(step.drag, 0, before)
        same(step.flips, [0, 1, 3, 4, 6], "a jump across the held item flips both ends")
    }

    function testAutoScroll() {
        check(Selection.autoScroll(500, 0, 1000, 100, 20) === 0, "no scrolling in the middle")
        check(Selection.autoScroll(100, 0, 1000, 100, 20) === 0 && Selection.autoScroll(900, 0, 1000, 100, 20) === 0,
              "zones end at their edge")
        check(Selection.autoScroll(50, 0, 1000, 100, 20) === -10, "half way into the top zone: half speed up")
        check(Selection.autoScroll(0, 0, 1000, 100, 20) === -20, "at the top edge: full speed up")
        check(Selection.autoScroll(-30, 0, 1000, 100, 20) === -20, "above the view: no faster")
        check(Selection.autoScroll(975, 0, 1000, 100, 20) === 15, "near the bottom: down")
        check(Selection.autoScroll(1200, 0, 1000, 100, 20) === 20, "over the panel below: full speed down")
        check(Selection.autoScroll(850, 0, 900, 100, 20) === 10, "the bottom is where the items end")
    }

    Component.onCompleted: {
        testSelectionStartEnd()
        testSelectionActions()
        testPanel()
        testMoreRows()
        testPanelParity()
        testDragRange()
        testDragSteps()
        testAutoScroll()
    }
}
