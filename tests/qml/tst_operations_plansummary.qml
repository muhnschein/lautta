// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/dialogs"

ApplicationWindow {
    id: test
    property bool reachedEnd

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function findAll(item, pred, out) {
        if (!item)
            return out
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem)
            findAll(item.contentItem, pred, out)
        return out
    }

    function one(item, name) {
        var all = findAll(item, function (x) { return x.objectName === name }, [])
        expect(all.length === 1, "one " + name + ", not " + all.length)
        return all[0]
    }

    function check(d) {
        var origin = one(d, "originTile")
        expect(origin.visible && origin.label === qsTrId("lautta-plan-origin"), "the origin tile is labelled Origin")
        expect(origin.name === qsTrId("lautta-plan-origin-more", 4).arg("IMG_0001.jpg"), "the origin counts the rest")
        expect(origin.stacked, "several items show as a stack")
        expect(origin.captions.length === 1 && origin.captions[0] === "DCIM", "the origin names its folder")
        expect(origin.look.thumbnailSource === "file:///DCIM/IMG_0001.jpg", "the origin shows its thumbnail")
        var destination = one(d, "destinationTile")
        expect(destination.label === qsTrId("lautta-plan-destination"), "the destination tile is labelled Destination")
        expect(destination.name === "photos" && destination.captions[0] === "srv", "the destination names itself and its folder")
        expect(destination.isDir, "the destination is a folder")
        expect(one(d, "originArrow").visible, "an arrow points from origin to destination")
        expect(one(d, "filesFigure").value === qsTrId("lautta-plan-files", 1284)
               && one(d, "filesFigure").label === qsTrId("lautta-plan-in-folders", 37), "files and folders are counted")
        expect(one(d, "freeFigure").visible, "free space is shown when known")
        var row = one(d, "conflictsRow")
        var title = one(d, "conflictsTitle")
        expect(row.visible && row.enabled && title.text === qsTrId("lautta-plan-conflict-count", 12),
               "open conflicts are counted")
        d.conflictCount = 0
        expect(row.visible && !row.enabled && title.text === qsTrId("lautta-plan-conflicts-resolved", 12),
               "resolved conflicts say so")
        expect(d.optionsJson() === JSON.stringify({ "suggestedNames": true }), "only the name choice is sent")
    }

    initialPage: Component {
        PlanSummaryDialog {
            planId: 1
            summary: ({
                "kind": "copy", "files": 1284, "dirs": 37, "bytes": 4500000000, "conflicts": 12, "renamed": 2,
                "destination": "lautta://nas/srv/photos", "destinationName": "photos", "destinationFolder": "srv",
                "origin": { "uri": "lautta://user-pictures/DCIM/IMG_0001.jpg", "name": "IMG_0001.jpg", "folder": "DCIM",
                            "more": 4, "isDir": false,
                            "look": { "category": "image", "thumbnailSource": "file:///DCIM/IMG_0001.jpg" } },
                "freeBytes": 812000000000, "permissionsSupported": true,
                "renames": [ { "from": "aux.txt", "to": "aux_.txt" }, { "from": "notes: draft.md", "to": "notes- draft.md" } ],
                "options": { "verifyChecksums": false, "preserveMtime": true, "preserveMode": false, "suggestedNames": true }
            })
            Component.onCompleted: {
                test.check(this)
                test.reachedEnd = true
            }
        }
    }
}
