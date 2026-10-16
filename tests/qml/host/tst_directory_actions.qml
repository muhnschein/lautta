// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of which actions an item offers where (ContextActions.js), SPEC
// §15.3: the long-press menu and the Details page's pulley.
import QtQuick 2.6
import "../../../qml/components/directory/ContextActions.js" as Actions

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function ids(entries) {
        var out = []
        for (var i = 0; i < entries.length; ++i)
            out.push(entries[i].id)
        return out
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function testMenu() {
        same(ids(Actions.menu({ "isDir": false, "writable": true })),
             ["share", "copy", "cut", "rename", "info", "delete"], "file in a writable folder")
        same(ids(Actions.menu({ "isDir": true, "writable": true })),
             ["copy", "cut", "rename", "info", "delete"], "folder: no Share")
        same(ids(Actions.menu({ "isDir": false, "writable": false })),
             ["share", "copy", "info"], "read-only folder: nothing that changes it")
        same(ids(Actions.menu({ "isDir": true, "writable": false })), ["copy", "info"], "read-only folder, folder")
    }

    function testDetails() {
        same(ids(Actions.details({ "isDir": false })), ["open_with", "compress"], "file")
        same(ids(Actions.details({ "isDir": true })), ["compress"], "folder: no Open with")
    }

    function testText() {
        // On the host qsTrId gives back the id.
        var entries = Actions.menu({ "isDir": false, "writable": true }).concat(Actions.details({ "isDir": false }))
        for (var i = 0; i < entries.length; ++i)
            check(entries[i].text.indexOf("lautta-dir-act-") === 0, entries[i].id + " has a translated text")
        same(Actions.text("info"), "lautta-dir-act-details", "Info is called Details")
        same(Actions.text("copy_to"), "copy_to", "Copy to… is gone")
    }

    function testGone() {
        var gone = ["copy_to", "move_to", "download", "upload_to", "extract", "favourite", "open_remote", "select"]
        var ctxs = [{ "isDir": false, "writable": true }, { "isDir": true, "writable": true }]
        for (var i = 0; i < gone.length; ++i) {
            for (var j = 0; j < ctxs.length; ++j)
                check(!Actions.applicable(gone[i], ctxs[j]), gone[i] + " is not offered")
        }
    }

    Component.onCompleted: {
        testMenu()
        testDetails()
        testText()
        testGone()
    }
}
