// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of which actions an item offers where (ContextActions.js), SPEC
// §15.3: the long-press menu, More actions last, and the rows of the More
// actions page, each with its theme icon (OPS-12). There is no Details page
// any more.
import QtQuick 2.6
import "../../../qml/components/directory/ContextActions.js" as Actions
import "../../../qml/components/FileIcons.js" as FileIcons

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
            same(ids(Actions.more(ctxs[i])),
                 ctxs[i].writable ? ["favourite", "compress", "moveToNewFolder"] : ["favourite", "compress"],
                 "More actions: " + JSON.stringify(ctxs[i]))
        }
        // Where the folder is not said (the image viewer's More page) it is
        // not offered: that page cannot follow the image into its new folder.
        same(ids(Actions.more({ "isDir": false })), ["favourite", "compress"], "More actions of the image viewer")
    }

    function testIcons() {
        var favourite = "image://theme/icon-m-favorite"
        var folder = "image://theme/icon-m-file-folder"
        same(Actions.more({ "isDir": false, "writable": true }).map(function(r) { return r.icon }),
             [favourite, FileIcons.archive, folder], "More actions icons, writable folder")
        same(Actions.more({ "isDir": true, "writable": false }).map(function(r) { return r.icon }),
             [favourite, FileIcons.archive], "More actions icons, read-only folder")
        same(Actions.icon("moveToNewFolder"), "image://theme/icon-m-file-folder", "Move to new folder")
        same(Actions.icon("favourite"), "image://theme/icon-m-favorite", "Add to favourites")
        same(Actions.icon("rename"), "image://theme/icon-m-edit", "Rename, a row for the selection's one item")
        same(Actions.icon("compress"), "image://theme/icon-m-file-archive-folder",
             "Compress: the archive icon, not the off-centre zipper badge")
        same(Actions.icon("copy"), "", "the menu's rows have no icon")
    }

    function testText() {
        // On the host qsTrId gives back the id.
        var entries = Actions.menu({ "isDir": false, "writable": true }).concat(Actions.more({ "isDir": false, "writable": true }))
        for (var i = 0; i < entries.length; ++i)
            check(entries[i].text.indexOf("lautta-dir-act-") === 0, entries[i].id + " has a translated text")
        same(Actions.text("more"), "lautta-dir-act-more", "More actions")
        same(Actions.text("favourite"), "lautta-dir-act-favourite", "Add to favourites")
        same(Actions.text("moveToNewFolder"), "lautta-dir-act-move-new-folder", "Move to new folder")
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
        testIcons()
        testText()
        testGone()
    }
}
