// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of when selection mode starts and ends (Selection.js), SPEC §15.3.
import QtQuick 2.6
import "../../../qml/components/directory/Selection.js" as Selection

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
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
        var ending = ["copy", "cut", "share", "compress"]
        for (var i = 0; i < ending.length; ++i)
            check(Selection.endsSelection(ending[i]), ending[i] + " ends selection")
        check(!Selection.endsSelection("delete"), "delete ends it only after the remorse timer")
        check(!Selection.endsSelection("copy_to") && !Selection.endsSelection(""), "other ids keep it")
    }

    Component.onCompleted: {
        testSelectionStartEnd()
        testSelectionActions()
    }
}
