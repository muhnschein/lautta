// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the context menu reordering (the `context_menu` setting) and
// of storing it through App.setSetting.
import QtQuick 2.6
import Lautta 1.0
import "../../../qml/components/search/ContextMenuOrder.js" as Order
import "../../../qml/components/search/ContextMenuLabels.js" as Labels

QtObject {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b) {
        return JSON.stringify(a) === JSON.stringify(b)
    }

    Component.onCompleted: {
        var base = ["a", "b", "c", "d", "e", "f", "g"]
        check(same(Order.moveUp(base, 2), ["a", "c", "b", "d", "e", "f", "g"]), "move up")
        check(same(Order.moveUp(base, 0), base), "the first entry cannot move up")
        check(same(Order.moveDown(base, 0), ["b", "a", "c", "d", "e", "f", "g"]), "move down")
        check(same(Order.moveDown(base, 6), base), "the last entry cannot move down")
        // Row: a..e, list: f, g.
        check(same(Order.toRow(base, 6), ["a", "b", "c", "d", "g", "e", "f"]),
              "a list entry becomes the last icon, the old last icon heads the list: " + JSON.stringify(Order.toRow(base, 6)))
        check(same(Order.toRow(base, 1), base), "entries of the row cannot move to the row")
        check(same(Order.toList(base, 0), ["b", "c", "d", "e", "f", "a", "g"]),
              "an icon heads the list, the old list head takes its place: " + JSON.stringify(Order.toList(base, 0)))
        check(same(Order.toList(base, 5), base), "entries of the list cannot move to the list")
        check(same(base, ["a", "b", "c", "d", "e", "f", "g"]), "inputs are not modified")
        for (var op = 0; op < 3; ++op) {
            var moved = [Order.moveUp, Order.toRow, Order.toList][op](base, 6 - op * 2)
            check(moved.slice().sort().join() === base.slice().sort().join(), "reordering keeps every entry")
        }

        check(same(Order.parse("not json"), Order.DEFAULT_ORDER), "bad text gives the default order")
        check(same(Order.parse("{}"), Order.DEFAULT_ORDER), "a non-list gives the default order")
        check(same(Order.parse("[\"x\"]"), ["x"]), "a stored order is kept")
        check(Order.DEFAULT_ORDER.length === 17 && Order.DEFAULT_ORDER[0] === "open_with", "default order")

        // Without a translation file qsTrId returns the id.
        check(Labels.label("copy_to") === "lautta-ctx-copy-to", "label id " + Labels.label("copy_to"))
        check(Labels.label("future_action") === "future_action", "unknown ids show as they are")
        check(Labels.icon("share") === "image://theme/icon-m-share", "icon")
        check(Labels.icon("future_action") === "image://theme/icon-m-other", "unknown icon")

        // Stored through App.setSetting; core keeps unknown ids and appends
        // missing known ones.
        var moved2 = Order.toRow(Order.DEFAULT_ORDER, 8)
        check(App.setSetting("context_menu", JSON.stringify(moved2)), "setting accepted")
        var stored = Order.parse(App.setting("context_menu"))
        check(same(stored, moved2), "order stored: " + JSON.stringify(stored))
        App.setSetting("context_menu", JSON.stringify(["info"]))
        stored = Order.parse(App.setting("context_menu"))
        check(stored[0] === "info" && stored.length === 17, "missing entries are appended: " + stored.length)
        App.setSetting("context_menu", JSON.stringify(Order.DEFAULT_ORDER))
        check(same(Order.parse(App.setting("context_menu")), Order.DEFAULT_ORDER), "default restored")
    }
}
