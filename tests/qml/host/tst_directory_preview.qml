// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    readonly property string samples: JSON.stringify([
        { "name": "Work", "dir": true, "modified": 5000 },
        { "name": "photo.jpg", "size": 2400000, "modified": 3000 },
        { "name": "notes.txt", "size": 3000, "modified": 4000 },
        { "name": ".backup", "size": 12000, "modified": 1000 },
        { "name": "Photos", "dir": true, "modified": 2000 }
    ])

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function names(sortKey, descending, foldersFirst, showHidden) {
        return App.previewRows(samples, sortKey, descending, foldersFirst, showHidden).map(function (r) {
            return r.name
        })
    }

    Component.onCompleted: {
        same(names("name", false, true, false), ["Photos", "Work", "notes.txt", "photo.jpg"],
             "name order, folders first, no hidden file")
        var rows = App.previewRows(samples, "name", false, true, false)
        var photo = rows[3]
        check(photo.category === "image" && photo.size === 2400000 && photo.modified === 3000 && !photo.isDir
              && !photo.hidden, "fields of the photo: " + JSON.stringify(photo))
        check(rows[0].isDir && rows[0].size === -1, "folders have no size")

        same(names("size", true, true, false), ["Photos", "Work", "photo.jpg", "notes.txt"], "largest file first")
        same(names("modified", false, false, true), [".backup", "Photos", "photo.jpg", "notes.txt", "Work"],
             "oldest first, folders mixed, hidden shown")
        check(App.previewRows(samples, "modified", false, false, true)[0].hidden, "the hidden file is a row")
        same(names("nonsense", false, false, true), [".backup", "notes.txt", "photo.jpg", "Photos", "Work"],
             "an unknown key sorts by name")
        check(App.previewRows("not a list", "name", false, true, false).length === 0,
              "bad entries make an empty folder")
    }
}
