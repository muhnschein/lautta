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
        same(ids(Actions.more({ "isDir": true, "changeable": false, "readable": false, "writable": true })), ["favourite"],
             "add only: no Compress")
        same(ids(Actions.menu({ "isDir": false, "changeable": true })),
             ["share", "copy", "cut", "rename", "delete", "more"], "readable unless said")
    }

    function testFavourite() {
        same(Actions.text("favourite"), "lautta-dir-act-favourite", "not a favourite: Add to favourites")
        same(Actions.text("favourite", { "favourite": true }), "lautta-dir-act-edit-favourite", "a favourite: Edit favourite")
        same(Actions.more({ "isDir": true, "favourite": true })[0].text, "lautta-dir-act-edit-favourite",
             "More actions of a favourite")
        same(Actions.more({ "isDir": true, "favourite": false })[0].text, "lautta-dir-act-favourite",
             "More actions of another folder")
    }

    function testMore() {
        var ctxs = [{ "isDir": false, "changeable": true, "writable": true }, { "isDir": true, "changeable": true, "writable": true },
                    { "isDir": false, "changeable": false, "writable": true }, { "isDir": true, "changeable": false, "writable": false }]
        var want = [["favourite", "compress", "checksums", "moveToNewFolder"], ["favourite", "compress", "moveToNewFolder"],
                    ["favourite", "compress", "checksums"], ["favourite"]]
        for (var i = 0; i < ctxs.length; ++i) {
            var menu = ids(Actions.menu(ctxs[i]))
            check(menu[menu.length - 1] === "more", "More actions is the last row: " + JSON.stringify(ctxs[i]))
            same(ids(Actions.more(ctxs[i])), want[i], "More actions: " + JSON.stringify(ctxs[i]))
        }
        same(ids(Actions.more({ "isDir": false, "writable": true })), ["favourite", "compress", "checksums"], "More actions of the image viewer")
        same(ids(Actions.more({ "isDir": false, "writable": false })), ["favourite", "checksums"], "image viewer, folder takes no files")
        same(ids(Actions.more({ "isDir": false })), ["favourite", "checksums"], "no Compress unless the folder takes files")
    }

    function testExtract() {
        var file = { "isDir": false, "changeable": true, "readable": true, "writable": true, "archive": true }
        same(ids(Actions.more(file)), ["favourite", "extract", "compress", "checksums", "moveToNewFolder"],
             "a compressed file: Extract before Compress")
        same(ids(Actions.menu(file)), ["share", "copy", "cut", "rename", "delete", "more"], "Extract only in More actions")
        same(ids(Actions.more({ "isDir": false, "changeable": false, "readable": true, "writable": true, "archive": true })),
             ["favourite", "extract", "compress", "checksums"], "read and add: Extract stays")
        same(ids(Actions.more({ "isDir": false, "changeable": false, "readable": true, "writable": false, "archive": true })),
             ["favourite", "checksums"], "folder takes no files: no Extract")
        same(ids(Actions.more({ "isDir": false, "changeable": false, "readable": false, "writable": true, "archive": true })),
             ["favourite"], "add only: nothing to read, no Extract")
        same(ids(Actions.more({ "isDir": false, "changeable": true, "readable": true, "writable": true, "archive": false })),
             ["favourite", "compress", "checksums", "moveToNewFolder"], "other files: no Extract")
        same(Actions.icon("extract"), "file-folder", "Extract: the folder glyph")
        same(Actions.text("extract"), "lautta-dir-act-extract", "Extract")
    }

    function testChecksums() {
        var list = { "isDir": false, "changeable": true, "readable": true, "writable": true, "checksumList": true }
        same(ids(Actions.more(list)), ["verify", "favourite", "compress", "checksums", "moveToNewFolder"],
             "a checksum list: Verify files first")
        same(ids(Actions.more({ "isDir": false, "readable": false, "checksumList": true })), ["favourite"],
             "add only: nothing to read, no Checksums or Verify files")
        same(ids(Actions.more({ "isDir": true, "changeable": true, "writable": true, "checksumList": true })),
             ["verify", "favourite", "compress", "moveToNewFolder"], "a folder: no Checksums")
        check(!Actions.applicable("verify", { "isDir": false }), "other files: no Verify files")
        same(ids(Actions.menu(list)), ["share", "copy", "cut", "rename", "delete", "more"], "both only in More actions")
        same(Actions.icon("checksums"), "checksum", "Checksums: the hash glyph")
        same(Actions.icon("verify"), "image://theme/icon-m-acknowledge", "Verify files: the tick")
        same(Actions.text("checksums"), "lautta-dir-act-checksums", "Checksums")
        same(Actions.text("verify"), "lautta-dir-act-verify", "Verify files")
    }

    function testIcons() {
        var favourite = "image://theme/icon-m-favorite"
        same(Actions.more({ "isDir": false, "changeable": true, "writable": true }).map(function(r) { return r.icon }),
             [favourite, "file-archive", "checksum", "file-folder"], "More actions icons, writable folder")
        same(Actions.more({ "isDir": true, "changeable": false, "writable": true }).map(function(r) { return r.icon }),
             [favourite, "file-archive"], "More actions icons, read and add")
        same(Actions.icon("moveToNewFolder"), "file-folder", "Move to new folder: the folder glyph")
        same(Actions.icon("favourite"), "image://theme/icon-m-favorite", "Add to favourites")
        same(Actions.icon("rename"), "image://theme/icon-m-edit", "Rename, a row for the selection's one item")
        same(Actions.icon("compress"), "file-archive", "Compress: the glyph archives show")
        same(Actions.icon("copy"), "", "the menu's rows have no icon")
    }

    function testText() {
        var full = { "isDir": false, "changeable": true, "writable": true, "archive": true }
        var entries = Actions.menu(full).concat(Actions.more(full))
        for (var i = 0; i < entries.length; ++i)
            check(entries[i].text.indexOf("lautta-dir-act-") === 0, entries[i].id + " has a translated text")
        same(Actions.text("more"), "lautta-dir-act-more", "More actions")
        same(Actions.text("favourite"), "lautta-dir-act-favourite", "Add to favourites")
        same(Actions.text("moveToNewFolder"), "lautta-dir-act-move-new-folder", "Move to new folder")
        same(Actions.text("info"), "info", "Details is gone")
        same(Actions.text("copy_to"), "copy_to", "Copy to… is gone")
    }

    function testGone() {
        var gone = ["info", "open_with", "copy_to", "move_to", "download", "upload_to", "open_remote", "select"]
        var ctxs = [{ "isDir": false, "changeable": true }, { "isDir": true, "changeable": true }]
        for (var i = 0; i < gone.length; ++i) {
            for (var j = 0; j < ctxs.length; ++j)
                check(!Actions.applicable(gone[i], ctxs[j]), gone[i] + " is not offered")
        }
        check(Actions.details === undefined, "no Details pulley")
    }

    Component.onCompleted: {
        testMenu()
        testFavourite()
        testMore()
        testExtract()
        testChecksums()
        testIcons()
        testText()
        testGone()
        reachedEnd = true
    }
}
