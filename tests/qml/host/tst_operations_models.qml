// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of BulkRenameModel (live preview), without Silica.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: test

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function uris(names) {
        return JSON.stringify(names.map(function(n) { return "lautta://user-documents/" + n }))
    }

    function rows(repeater) {
        var out = []
        for (var i = 0; i < repeater.count; ++i) {
            var r = repeater.itemAt(i)
            out.push([r.oldName, r.newName, r.state])
        }
        return out
    }

    BulkRenameModel {
        id: bulk
    }

    Repeater {
        id: preview
        model: bulk
        delegate: Item {
            property string oldName: model.old
            property string newName: model.new
            property string state: model.status
        }
    }

    BulkRenameModel {
        id: other
    }

    Component.onCompleted: {
        // Find and replace with numbering.
        bulk.urisJson = uris(["IMG_1.jpg", "IMG_2.jpg", "keep.jpg"])
        bulk.rulesJson = JSON.stringify({ "rules": [
            { "type": "findReplace", "find": "IMG_", "replace": "Trip-", "regex": false, "caseSensitive": true }
        ] })
        check(bulk.count === 3, "three rows: " + bulk.count)
        var r = rows(preview)
        check(r[0][0] === "IMG_1.jpg" && r[0][1] === "Trip-1.jpg" && r[0][2] === "ok", "first row: " + r[0])
        check(r[2][2] === "unchanged", "untouched names are unchanged: " + r[2])
        check(bulk.changedCount === 2 && bulk.problemCount === 0, "counts")
        check(bulk.errorKind === "", "no error")

        // The preview follows the rules live.
        bulk.rulesJson = JSON.stringify({ "rules": [
            { "type": "numbering", "start": 7, "step": 1, "padding": 3, "position": "suffix", "separator": "-" }
        ] })
        r = rows(preview)
        check(r[0][1] === "IMG_1-007.jpg" && r[1][1] === "IMG_2-008.jpg", "numbering: " + r[0][1] + " " + r[1][1])

        // Names that collide are flagged.
        bulk.urisJson = uris(["a1.txt", "a2.txt"])
        bulk.rulesJson = JSON.stringify({ "rules": [
            { "type": "findReplace", "find": "[12]", "replace": "", "regex": true, "caseSensitive": true }
        ] })
        r = rows(preview)
        check(r[0][2] === "collision" && r[1][2] === "collision", "collisions: " + r[0][2])
        check(bulk.problemCount === 2, "problem count")

        // A broken pattern is reported and the preview is kept.
        bulk.rulesJson = JSON.stringify({ "rules": [
            { "type": "findReplace", "find": "(", "replace": "", "regex": true, "caseSensitive": true }
        ] })
        check(bulk.errorKind === "InvalidArgument", "bad regex reported: " + bulk.errorKind)
        bulk.rulesJson = "not json"
        check(bulk.errorKind === "InvalidArgument", "bad rules reported")
        check(bulk.count === 2, "rows stay")

        // Nothing selected, nothing to preview.
        other.urisJson = "[]"
        other.rulesJson = JSON.stringify({ "rules": [] })
        check(other.count === 0 && !other.busy, "empty selection")
        other.apply()
        check(!other.busy, "applying nothing does nothing")
    }
}
