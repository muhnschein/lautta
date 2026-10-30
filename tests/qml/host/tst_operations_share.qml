// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/operations/Share.js" as Share

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    Component.onCompleted: {
        same(Share.remember([], "a"), ["a"], "first save")
        same(Share.remember(["b", "a"], "a"), ["a", "b"], "a repeat moves to the top, once")
        same(Share.remember(["b", "c", "d"], "a"), ["a", "b", "c"], "three kept")
        same(Share.remember(["a", "b", "c"], "a"), ["a", "b", "c"], "the newest again")

        var places = [{ "uri": "lautta://user-documents/" }, { "uri": "lautta://user-downloads/" },
                      { "uri": "lautta://nas/" }]
        // A throw in Share.js only logs a note against Share.js: catch it here.
        var kept
        try {
            kept = Share.usable(["lautta://user-documents/Uni", "lautta://gone/x", 3, "lautta://nas2/y",
                                 "lautta://gone/lautta://nas/", "lautta://nas/"], places)
        } catch (e) {
            kept = String(e)
        }
        same(kept, ["lautta://user-documents/Uni", "lautta://nas/"], "folders of gone or read-only places and junk drop out")
        same(Share.start(["lautta://nas/a", "lautta://user-documents/"], places), "lautta://nas/a",
             "starts in the last folder saved to")
        same(Share.start([], places), "lautta://user-downloads/", "first time: Downloads")
        same(Share.start([], [{ "uri": "lautta://user-documents/" }]), "lautta://user-documents/",
             "no Downloads: the first place")
        same(Share.start([], []), "", "no place at all")

        // Host has no catalogue: qsTrId gives the id; %n and %1 still fill in.
        var save = qsTrId("lautta-share-count", 2)
        same(Share.note(2, []), save, "all readable")
        same(Share.note(2, ["notes.pdf"]), save + " · " + qsTrId("lautta-share-skipped-one").arg("notes.pdf"),
             "one skipped, named")
        same(Share.note(1, ["a", "b"]), qsTrId("lautta-share-count", 1) + " · " + qsTrId("lautta-share-skipped-many", 2),
             "several skipped, counted")
        same(Share.note(0, ["a"]), qsTrId("lautta-share-skipped-one").arg("a"), "nothing to save: only what's skipped")
        same(Share.note(0, []), "", "nothing at all")

        same(Share.locationIcon("volume"), "image://theme/icon-m-sd-card", "memory card")
        same(Share.locationIcon("server"), "server", "server")
        same(Share.locationIcon("folder"), "file-folder", "user folder")
        same(Share.locationIcon("android"), "file-folder", "Android storage")
    }
}
