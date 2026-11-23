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

    function named(item, name) {
        return findAll(item, function (x) { return x.objectName === name }, [])
    }

    function one(item, name) {
        var all = named(item, name)
        expect(all.length === 1, "one " + name + ", not " + all.length)
        return all[0]
    }

    function checkPlanConflict(d) {
        expect(d.choice === "KeepBoth", "the preselected choice is Keep both, not " + d.choice)
        expect(d.canAccept, "a preselected choice can be accepted")
        expect(one(d, "conflictPosition").text === qsTrId("lautta-conflict-position").arg(3).arg(12),
               "the header says which conflict of how many")
        var incoming = one(d, "incomingTile")
        var existing = one(d, "existingTile")
        expect(incoming.name === "IMG_2041.jpg" && existing.name === "IMG_2041.jpg", "both tiles show the name")
        expect(incoming.label === qsTrId("lautta-conflict-copying"), "the incoming tile says Copying")
        expect(existing.label === qsTrId("lautta-conflict-already-there"), "the existing tile says Already there")
        expect(incoming.note === qsTrId("lautta-conflict-newer") && existing.note === "", "only the newer side says so")
        expect(incoming.look.thumbnailSource === "file:///src/IMG_2041.jpg", "the incoming tile shows its thumbnail")
        expect(existing.look.thumbnailSource === "file:///dst/IMG_2041.jpg", "the existing tile shows its thumbnail")
        expect(named(d, "choiceMerge").length === 0, "Merge is not listed for files")
        expect(!one(d, "choiceResume").enabled, "a choice not offered is shown dimmed")
        expect(one(d, "choiceReplace").enabled, "an offered choice can be picked")
        expect(one(d, "choiceKeepBoth").checked, "the picked choice is checked")
        one(d, "choiceSkip").clicked(null)
        expect(d.choice === "Skip" && one(d, "choiceSkip").checked && !one(d, "choiceKeepBoth").checked,
               "choices are exclusive")
        var applyAll = one(d, "applyAll")
        expect(applyAll.visible && applyAll.text === qsTrId("lautta-conflict-apply-all", 9),
               "apply-all counts the conflicts after this one")
    }

    function checkTransferConflict(d) {
        expect(one(d, "conflictPosition").text === qsTrId("lautta-conflict-one"), "a lone conflict has a plain header")
        var incoming = one(d, "incomingTile")
        expect(incoming.name === "report.pdf" && one(d, "existingTile").name === "report.pdf",
               "a running transfer's conflict shows its name")
        expect(incoming.label === qsTrId("lautta-conflict-moving"), "a move says Moving")
        expect(incoming.uri === "lautta://user-documents/report.pdf", "the incoming tile knows its file")
        expect(incoming.look.category === "pdf", "the incoming tile shows its look")
        expect(!one(d, "applyAll").visible, "no apply-all for the last conflict")
    }

    Component {
        id: transferConflict

        ConflictDialog {
            transferId: 4
            item: 0
            conflict: ({
                "name": "report.pdf", "kind": "move", "open": 1,
                "source": "lautta://user-documents/report.pdf", "destination": "lautta://user-downloads/report.pdf",
                "source_look": { "category": "pdf", "mimeType": "application/pdf" }, "destination_look": {},
                "dst_is_dir": false, "src_is_dir": false, "src_size": 10, "dst_size": 20,
                "choices": [ "KeepBoth", "Skip", "Replace" ]
            })
        }
    }

    initialPage: Component {
        ConflictDialog {
            planId: 1
            item: 3
            name: "IMG_2041.jpg"
            position: 3
            total: 12
            defaultChoice: "KeepBoth"
            sourceLook: ({ "category": "image", "thumbnailSource": "file:///src/IMG_2041.jpg" })
            destinationLook: ({ "category": "image", "thumbnailSource": "file:///dst/IMG_2041.jpg" })
            conflict: ({
                "dst_is_dir": false, "src_is_dir": false, "src_size": 3100000, "dst_size": 2800000,
                "src_mtime_ms": 1790000000000, "dst_mtime_ms": 1789000000000, "resumable": false,
                "choices": [ "KeepBoth", "Skip", "Replace", "ReplaceIfNewer" ]
            })
            Component.onCompleted: {
                test.checkPlanConflict(this)
                test.checkTransferConflict(transferConflict.createObject(test))
                test.reachedEnd = true
            }
        }
    }
}
