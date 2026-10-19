// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of which actions an item offers where (ContextActions.js), SPEC
// §15.3: the long-press menu, More actions last, and what More actions puts
// in the menu's place. There is no Details page any more.
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
             ["share", "copy", "cut", "rename", "delete", "more"], "file in a writable folder")
        same(ids(Actions.menu({ "isDir": true, "writable": true })),
             ["copy", "cut", "rename", "delete", "more"], "folder: no Share")
        same(ids(Actions.menu({ "isDir": false, "writable": false })),
             ["share", "copy", "more"], "read-only folder: nothing that changes it")
        same(ids(Actions.menu({ "isDir": true, "writable": false })), ["copy", "more"], "read-only folder, folder")
    }

    function testMore() {
        var ctxs = [{ "isDir": false, "writable": true }, { "isDir": true, "writable": true },
                    { "isDir": false, "writable": false }, { "isDir": true, "writable": false }]
        for (var i = 0; i < ctxs.length; ++i) {
            var menu = ids(Actions.menu(ctxs[i]))
            check(menu[menu.length - 1] === "more", "More actions is the last row: " + JSON.stringify(ctxs[i]))
            same(ids(Actions.more(ctxs[i])), ["favourite", "compress"], "More actions: " + JSON.stringify(ctxs[i]))
        }
    }

    function testText() {
        // On the host qsTrId gives back the id.
        var entries = Actions.menu({ "isDir": false, "writable": true }).concat(Actions.more({ "isDir": false }))
        for (var i = 0; i < entries.length; ++i)
            check(entries[i].text.indexOf("lautta-dir-act-") === 0, entries[i].id + " has a translated text")
        same(Actions.text("more"), "lautta-dir-act-more", "More actions")
        same(Actions.text("favourite"), "lautta-dir-act-favourite", "Add to favourites")
        same(Actions.text("info"), "info", "Details is gone")
        same(Actions.text("copy_to"), "copy_to", "Copy to… is gone")
    }

    function testGone() {
        var gone = ["info", "open_with", "copy_to", "move_to", "download", "upload_to", "extract", "open_remote", "select"]
        var ctxs = [{ "isDir": false, "writable": true }, { "isDir": true, "writable": true }]
        for (var i = 0; i < gone.length; ++i) {
            for (var j = 0; j < ctxs.length; ++j)
                check(!Actions.applicable(gone[i], ctxs[j]), gone[i] + " is not offered")
        }
        check(Actions.details === undefined, "no Details pulley")
    }

    Component.onCompleted: {
        testMenu()
        testMore()
        testText()
        testGone()
    }
}
