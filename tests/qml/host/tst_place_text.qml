// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of PlaceText.around (§15.5): a line that names a location is
// split around the name wherever its translation puts it, so that a
// server's name gets its icon right before it; a line that does not name
// it is left whole.
import QtQuick 2.6
import "../../../qml/components/PlaceText.js" as PlaceText

QtObject {
    function same(got, want, what) {
        var g = JSON.stringify(got)
        var w = JSON.stringify(want)
        if (g !== w)
            console.error("FAILED: " + what + ": " + g + ", not " + w)
    }

    function line(format) {
        return function (name) { return format.arg(name) }
    }

    Component.onCompleted: {
        same(PlaceText.around(line("Copied 4 items to %1"), "NAS"),
             { "before": "Copied 4 items to ", "rest": "NAS", "named": true }, "the name at the end")
        same(PlaceText.around(line("%1: 4 items copied"), "NAS"),
             { "before": "", "rest": "NAS: 4 items copied", "named": true }, "the name first")
        same(PlaceText.around(line("Copied to %1 at last"), "Office NAS"),
             { "before": "Copied to ", "rest": "Office NAS at last", "named": true }, "the name inside")
        same(PlaceText.around(function () { return "Deleted 4 items" }, "NAS"),
             { "before": "", "rest": "Deleted 4 items", "named": false }, "a line without the name")
        same(PlaceText.around(line("Copied to %1"), ""),
             { "before": "", "rest": "Copied to ", "named": false }, "no name to mark")
    }
}
