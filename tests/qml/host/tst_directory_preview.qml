// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of ViewPreviewModel (no Silica): the sample folder of the view
// options preview (BRW-4) is ordered and filtered as a real folder, and
// follows every change of its view settings at once.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function shown() {
        var out = []
        for (var i = 0; i < rows.count; ++i)
            out.push(rows.itemAt(i).nm)
        return out
    }

    ViewPreviewModel {
        id: samples

        entries: JSON.stringify([
            { "name": "Work", "dir": true, "modified": 5000 },
            { "name": "photo.jpg", "size": 2400000, "modified": 3000 },
            { "name": "notes.txt", "size": 3000, "modified": 4000 },
            { "name": ".backup", "size": 12000, "modified": 1000 },
            { "name": "Photos", "dir": true, "modified": 2000 }
        ])
        sortKey: "name"
        foldersFirst: true
    }

    Repeater {
        id: rows

        model: samples
        delegate: Item {
            property string nm: name
            property bool folder: isDir
            property real bytes: size
            property real when: modified
            property bool dot: hidden
            property string cat: category
        }
    }

    Component.onCompleted: {
        same(samples.names, ["Photos", "Work", "notes.txt", "photo.jpg"], "name order, folders first, no hidden file")
        same(shown(), samples.names, "the rows are in that order")
        check(samples.count === 4, "count")
        var photo = rows.itemAt(3)
        check(photo.cat === "image" && photo.bytes === 2400000 && photo.when === 3000 && !photo.folder,
              "roles of the photo")
        check(rows.itemAt(0).folder && rows.itemAt(0).bytes === -1, "folders have no size")

        samples.sortKey = "size"
        samples.descending = true
        same(shown(), ["Photos", "Work", "photo.jpg", "notes.txt"], "largest file first")

        samples.foldersFirst = false
        samples.showHidden = true
        samples.sortKey = "modified"
        samples.descending = false
        same(shown(), [".backup", "Photos", "photo.jpg", "notes.txt", "Work"], "oldest first, folders mixed, hidden shown")
        check(rows.itemAt(0).dot && samples.count === 5, "the hidden file is a row")

        samples.sortKey = "nonsense"
        same(shown(), [".backup", "notes.txt", "photo.jpg", "Photos", "Work"], "an unknown key sorts by name")

        samples.entries = "not a list"
        check(samples.count === 0 && rows.count === 0, "bad entries make an empty folder")
    }
}
