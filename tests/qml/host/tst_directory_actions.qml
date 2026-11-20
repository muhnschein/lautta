// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/directory/ContextActions.js" as Actions

Item {
    property bool reachedEnd
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
        same(ids(Actions.menu({ "isDir": false, "changeable": true, "readable": true })),
             ["share", "copy", "cut", "rename", "delete", "more"], "file in a writable folder")
        same(ids(Actions.menu({ "isDir": true, "changeable": true, "readable": true })),
             ["copy", "cut", "rename", "delete", "more"], "folder: no Share")
        same(ids(Actions.menu({ "isDir": false, "changeable": false, "readable": true })),
             ["share", "copy", "more"], "read-only folder, read and add: nothing that changes items")
        same(ids(Actions.menu({ "isDir": true, "changeable": false, "readable": true })), ["copy", "more"],
             "read-only folder, folder")
        same(ids(Actions.menu({ "isDir": true, "changeable": false, "readable": false })), ["more"],
             "add only: nothing to read; Add to favourites stays")
        same(ids(Actions.more({ "isDir": true, "changeable": false, "readable": false })), ["favourite"],
             "add only: no Compress")
        same(ids(Actions.menu({ "isDir": false, "changeable": true })),
             ["share", "copy", "cut", "rename", "delete", "more"], "readable unless said")
    }

    function testMore() {
        var ctxs = [{ "isDir": false, "changeable": true }, { "isDir": true, "changeable": true },
                    { "isDir": false, "changeable": false }, { "isDir": true, "changeable": false }]
        for (var i = 0; i < ctxs.length; ++i) {
            var menu = ids(Actions.menu(ctxs[i]))
            check(menu[menu.length - 1] === "more", "More actions is the last row: " + JSON.stringify(ctxs[i]))
            same(ids(Actions.more(ctxs[i])),
                 ctxs[i].changeable ? ["favourite", "compress", "moveToNewFolder"] : ["favourite", "compress"],
                 "More actions: " + JSON.stringify(ctxs[i]))
        }
        same(ids(Actions.more({ "isDir": false })), ["favourite", "compress"], "More actions of the image viewer")
    }

    function testIcons() {
        var favourite = "image://theme/icon-m-favorite"
        same(Actions.more({ "isDir": false, "changeable": true }).map(function(r) { return r.icon }),
             [favourite, "file-archive", "file-folder"], "More actions icons, writable folder")
        same(Actions.more({ "isDir": true, "changeable": false }).map(function(r) { return r.icon }),
             [favourite, "file-archive"], "More actions icons, read-only folder")
        same(Actions.icon("moveToNewFolder"), "file-folder", "Move to new folder: the folder glyph")
        same(Actions.icon("favourite"), "image://theme/icon-m-favorite", "Add to favourites")
        same(Actions.icon("rename"), "image://theme/icon-m-edit", "Rename, a row for the selection's one item")
        same(Actions.icon("compress"), "file-archive", "Compress: the glyph archives show")
        same(Actions.icon("copy"), "", "the menu's rows have no icon")
    }

    function testText() {
        var entries = Actions.menu({ "isDir": false, "changeable": true }).concat(Actions.more({ "isDir": false, "changeable": true }))
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
        var ctxs = [{ "isDir": false, "changeable": true }, { "isDir": true, "changeable": true }]
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
        reachedEnd = true
    }
}
