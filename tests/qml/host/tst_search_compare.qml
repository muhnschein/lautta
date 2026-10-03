// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of CompareModel and SyncPairsModel (SYN-1..3): two temporary
// folders with differences, preview counts, exclusions, a sync run and a
// re-compare, saved pairs.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property int started: -1
    property int savedId: 0
    property string failure: ""

    SearchTestSupport { id: support }
    CompareModel {
        id: cmp
        leftUri: "lautta://user-documents/srch_cmp_l"
        rightUri: "lautta://user-documents/srch_cmp_r"
        excludesText: "*.tmp"
        onSyncStarted: root.started = count
        onSyncFailed: root.failure = kind
        onPairSaved: root.savedId = id
    }
    CompareModel { id: loaded }
    SyncPairsModel { id: pairs }
    Repeater {
        id: rows
        model: cmp
        delegate: Item {
            property string rowGroup: group
            property string rowRel: rel
            property string rowStatus: status
            property bool rowExcluded: excluded
            property real rowSize: size
            property real rowModified: modified
        }
    }
    Repeater {
        id: pairRows
        model: pairs
        delegate: Item {
            property string rowLabel: label
            property string rowLeft: leftUri
            property int rowId: pairId
        }
    }

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function until(cond, ms) {
        for (var i = 0; i < ms / 10 && !cond(); ++i)
            support.spin(10)
        return cond()
    }

    function compare() {
        check(cmp.compare(), "compare starts")
        check(until(function () { return cmp.ready && !cmp.running }, 5000), "compare finished: " + cmp.errorKind)
    }

    function groupOf(rel) {
        for (var i = 0; i < rows.count; ++i)
            if (rows.itemAt(i).rowRel === rel)
                return rows.itemAt(i).rowGroup
        return ""
    }

    function indexOfRel(rel) {
        for (var i = 0; i < rows.count; ++i)
            if (rows.itemAt(i).rowRel === rel)
                return i
        return -1
    }

    Component.onCompleted: {
        support.prepare()
        var t = 1700000000
        support.mkdir("Documents/srch_cmp_l")
        support.mkdir("Documents/srch_cmp_r")
        support.write("Documents/srch_cmp_l/same.txt", "same", t)
        support.write("Documents/srch_cmp_r/same.txt", "same", t)
        support.write("Documents/srch_cmp_l/only_left.txt", "L", t)
        support.write("Documents/srch_cmp_r/only_right.txt", "R", t)
        support.write("Documents/srch_cmp_l/newer.txt", "new!", t + 5000)
        support.write("Documents/srch_cmp_r/newer.txt", "old", t)
        support.write("Documents/srch_cmp_l/sub/deep.txt", "deep", t)
        support.write("Documents/srch_cmp_l/skip.tmp", "t", t)

        cmp.mode = "mirror_lr"
        compare()
        check(cmp.totalCount === 6, "items compared (skip.tmp excluded): " + cmp.totalCount)
        check(cmp.sameCount === 1, "same " + cmp.sameCount)
        check(cmp.leftOnlyCount === 3, "left only (incl. sub and sub/deep): " + cmp.leftOnlyCount)
        check(cmp.rightOnlyCount === 1, "right only " + cmp.rightOnlyCount)
        check(cmp.leftNewerCount === 1, "left newer " + cmp.leftNewerCount)
        check(cmp.copyCount === 3, "mirror copies new.txt, only_left, deep: " + cmp.copyCount)
        check(cmp.newFolderCount === 1, "creates sub: " + cmp.newFolderCount)
        check(cmp.replaceCount === 1, "replaces newer.txt: " + cmp.replaceCount)
        check(cmp.deleteCount === 1, "deletes only_right: " + cmp.deleteCount)
        check(cmp.syncCount === 5, "sync count " + cmp.syncCount)
        check(groupOf("only_left.txt") === "copy_right", "group of only_left: " + groupOf("only_left.txt"))
        check(groupOf("newer.txt") === "replace_right", "group of newer")
        check(groupOf("only_right.txt") === "delete_right", "group of only_right")
        check(groupOf("same.txt") === "", "same items are not listed")
        check(cmp.count === 5, "rows " + cmp.count)

        var preview = JSON.parse(cmp.syncPreviewJson("update_both"))
        check(preview.deletes === 0 && preview.copyFiles === 4 && preview.replaced === 1,
              "update-both preview: " + JSON.stringify(preview))
        preview = JSON.parse(cmp.syncPreviewJson("mirror_rl"))
        check(preview.deletes === 3 && preview.copyFiles === 2,
              "mirror right to left preview: " + JSON.stringify(preview))

        // Mode switch regroups; per-item exclusion changes the counts.
        cmp.mode = "update_both"
        check(cmp.deleteCount === 0, "update both never deletes")
        check(groupOf("only_right.txt") === "copy_left", "only_right is copied left: " + groupOf("only_right.txt"))
        var before = cmp.syncCount
        var idx = indexOfRel("sub")
        check(idx >= 0, "sub listed")
        cmp.toggleExcluded(idx)
        check(cmp.syncCount === before - 2, "excluding a folder drops it and its children: " + cmp.syncCount)
        var i2 = indexOfRel("sub/deep.txt")
        check(rows.itemAt(i2).rowExcluded, "children show as excluded")
        cmp.toggleExcluded(indexOfRel("sub"))
        check(cmp.syncCount === before, "toggling again restores")

        // Save as a pair and read it back.
        cmp.saveAsPair("Docs test")
        check(until(function () { return root.savedId > 0 }, 3000), "pair saved")
        check(cmp.pairId === root.savedId, "pair id kept")
        pairs.reload()
        check(until(function () { return pairs.count === 1 }, 3000), "one pair listed")
        check(pairRows.itemAt(0).rowLabel === "Docs test", "pair label")
        check(pairRows.itemAt(0).rowLeft === "lautta://user-documents/srch_cmp_l", "pair left")
        loaded.loadPair(root.savedId)
        check(until(function () { return loaded.pairId === root.savedId }, 3000), "pair loaded")
        check(loaded.mode === "update_both" && loaded.excludesText === "*.tmp", "loaded options " + loaded.mode + " " + loaded.excludesText)
        check(loaded.leftUri === cmp.leftUri && loaded.rightUri === cmp.rightUri, "loaded folders")
        cmp.saveAsPair("Renamed")
        check(until(function () {
            pairs.reload()
            support.spin(50)
            return pairRows.count === 1 && pairRows.itemAt(0).rowLabel === "Renamed"
        }, 3000), "saving again updates the pair")
        pairs.remove(pairRows.itemAt(0).rowId)
        check(until(function () { return pairs.count === 0 }, 3000), "pair removed")

        // The row shows the side a copy reads from.
        var left = rows.itemAt(indexOfRel("only_left.txt"))
        check(left.rowSize === 1 && left.rowModified === 1700000000000, "row size and date: " + left.rowSize + " " + left.rowModified)
        var right = rows.itemAt(indexOfRel("only_right.txt"))
        check(right.rowSize === 1, "right-only row size " + right.rowSize)

        // Sync with one item excluded: it stays untouched.
        cmp.toggleExcluded(indexOfRel("only_right.txt"))
        check(cmp.sync("update_both"), "sync starts")
        check(until(function () { return root.started >= 0 }, 3000), "sync queued: " + root.failure)
        check(root.started === 1, "only the copies to the right remain: " + root.started)
        // The SDK runs the binary under emulation, where local file copies
        // fail; the transfer engine is covered by the core tests there.
        if (!support.emulated()) {
            var same = false
            for (var n = 0; n < 100 && !same; ++n) {
                support.spin(100)
                cmp.compare()
                until(function () { return cmp.ready && !cmp.running }, 3000)
                same = cmp.totalCount - cmp.sameCount === 1
            }
            check(same, "only the excluded item differs after the sync: " + cmp.sameCount + "/" + cmp.totalCount + " " + support.transferStates())
            check(!support.exists("Documents/srch_cmp_l/only_right.txt"), "excluded item not copied")
            check(support.exists("Documents/srch_cmp_r/sub/deep.txt"), "folder copied")
            check(support.read("Documents/srch_cmp_r/newer.txt") === "new!", "newer file replaced the older")
            check(!support.exists("Documents/srch_cmp_r/skip.tmp"), "pattern-excluded file not synced")

            // Without the exclusion the second run finishes the job.
            root.started = -1
            check(cmp.sync("update_both"), "second sync starts")
            check(until(function () { return root.started >= 0 }, 3000), "second sync queued")
            same = false
            for (var m = 0; m < 100 && !same; ++m) {
                support.spin(100)
                cmp.compare()
                until(function () { return cmp.ready && !cmp.running }, 3000)
                same = cmp.totalCount === cmp.sameCount
            }
            check(same, "everything is the same: " + cmp.sameCount + "/" + cmp.totalCount)
            check(cmp.syncCount === 0 && cmp.count === 0, "nothing left to sync")
            check(support.exists("Documents/srch_cmp_l/only_right.txt"), "right-only file copied left")
        }

        // A missing side is reported.
        cmp.rightUri = "lautta://user-documents/srch_cmp_missing"
        cmp.compare()
        until(function () { return !cmp.running && cmp.errorKind.length > 0 }, 3000)
        check(cmp.errorKind.length > 0 && !cmp.ready, "missing folder reports an error: " + cmp.errorKind)
    }
}
