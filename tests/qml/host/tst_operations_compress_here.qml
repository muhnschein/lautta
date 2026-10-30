// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0
import "../../../qml/components/operations/Compress.js" as Compress

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    Component.onCompleted: {
        same(Compress.archiveName(["report.pdf"], false), "report", "one file: its name without extension")
        same(Compress.archiveName(["data.tar.gz"], false), "data", "double extension goes too")
        same(Compress.archiveName(["v1.2"], true), "v1.2", "one folder: its whole name")
        var several = qsTrId("lautta-compress-default-name")
        same(Compress.archiveName(["a.txt", "b"], false), several, "several: Archive")
        same(Compress.archiveName(["a.txt", "b"], true), several, "several folders: Archive")

        var calls = []
        var ops = { "compress": function (urisJson, dest, name, kind) { calls.push([urisJson, dest, name, kind]) } }
        Compress.start(App, ops, ["lautta://user-documents/Uni/notes.txt"], false)
        same(calls[0], [JSON.stringify(["lautta://user-documents/Uni/notes.txt"]), "lautta://user-documents/Uni",
                        "notes", "zip"], "beside the item, as zip")
        Compress.start(App, ops, ["lautta://user-documents/a", "lautta://user-documents/b"], true)
        same(calls[1].slice(1), ["lautta://user-documents/", several, "zip"], "several items: Archive.zip beside them")
    }
}
