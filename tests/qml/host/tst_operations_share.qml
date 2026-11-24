// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0
import "../../../qml/components/operations/Share.js" as Share

Item {
    id: root
    property bool reachedEnd

    readonly property string downloads: "lautta://user-downloads/"
    readonly property string documents: "lautta://user-documents/"

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function places() {
        return JSON.parse(Operations.sharePlacesJson())
    }

    function kept() {
        return JSON.parse(App.settingsJson).share_folders
    }

    function saveTo(folder) {
        check(App.setSetting("share_folders", JSON.stringify(Share.remembered(folder, kept()))),
              "the folder saved to is kept: " + folder)
    }

    function row(name) {
        for (var i = 0; i < rows.count; ++i) {
            if (rows.itemAt(i).name === name)
                return rows.itemAt(i)
        }
        check(false, "a place called " + name)
        return null
    }

    function testShared() {
        var shared = Share.paths(["file:///a/one.jpg", { "filePath": "/a/two.txt" }, { "url": "file:///a/three.pdf" }, {}])
        same(shared, ["file:///a/one.jpg", "/a/two.txt", "file:///a/three.pdf", ""], "a path of each shared thing")
        same(Share.paths([{ "filePath": "/a/four.txt", "url": "file:///b/five.txt" }]), ["/a/four.txt"],
             "a file path leads over an address")
        var entries = Share.probed(shared, [
            { "name": "one.jpg", "uri": "lautta://x/one.jpg", "readable": true, "size": 1, "path": "/a/one.jpg" },
            { "name": "two.txt", "uri": "", "readable": false, "size": 0, "path": "/a/two.txt" },
            { "name": "three.pdf", "uri": "lautta://x/three.pdf", "readable": true, "size": 3, "path": "/a/three.pdf" },
            { "name": "", "uri": "lautta://x/", "readable": true, "size": 0, "path": "" }
        ])
        same(entries.map(function (e) { return e.readable }), [true, false, true, false],
             "a thing shared without a path can't be read")
        same(Share.readable(entries).map(function (e) { return e.name }), ["one.jpg", "three.pdf"], "files to save")
        same(Share.skipped(entries).map(function (e) { return e.name }), ["two.txt", ""], "files left out")
        same(Share.uris(Share.readable(entries)), ["lautta://x/one.jpg", "lautta://x/three.pdf"], "what is copied")
        same(Share.readable([]), [], "nothing shared, nothing to save")
    }

    function testRemembered() {
        same(Share.remembered("b", ["a"]), ["b", "a"], "latest first")
        same(Share.remembered("a", undefined), ["a"], "nothing kept before")
    }

    function testCanSave() {
        var here = "lautta://x/a"
        check(Share.canSave(1, here, true, false, ""), "a file, a listed folder that takes files")
        check(!Share.canSave(0, here, true, false, ""), "not without a file that can be read")
        check(!Share.canSave(1, "", true, false, ""), "not on Locations")
        check(!Share.canSave(1, here, false, false, ""), "not where nothing can be added")
        check(!Share.canSave(1, here, true, true, ""), "not before the folder is listed")
        check(!Share.canSave(1, here, true, false, "NotFound"), "not in a folder that failed to open")
    }

    function testSections() {
        same(["folder", "android", "volume", "server"].map(Share.sectionOf), ["device", "android", "volumes", "servers"],
             "places group as on Home")
    }

    function testPlaces() {
        same(places(), { "start": downloads, "recent": [] }, "first time: Downloads, nothing saved lately")
        saveTo(documents)
        var once = places()
        same(once.start, documents, "then where it saved last")
        same(once.recent, [{ "uri": documents, "name": "Documents", "place": "", "remote": false }], "one folder saved to")
        saveTo(downloads)
        saveTo(documents)
        same(places().recent.map(function (f) { return f.name }), ["Documents", "Downloads"],
             "a folder saved to again moves up, listed once")
        same(kept(), [documents, downloads], "kept as they are listed")
        same(JSON.parse(Operations.sharePlacesJson()).start, documents, "start follows")
    }

    function testRoots() {
        var place = row("Downloads")
        if (!place)
            return
        same([place.uri, place.kind, place.icon, place.writable, place.selectable],
             [downloads, "folder", "file-folder", true, true], "a folder on the device")
    }

    PickerRootsModel { id: roots }

    Repeater {
        id: rows

        model: roots

        Item {
            readonly property string uri: model.uri
            readonly property string name: model.name
            readonly property string kind: model.kind
            readonly property string icon: model.icon
            readonly property bool writable: model.writable
            readonly property bool selectable: model.selectable
        }
    }

    Component.onCompleted: {
        testShared()
        testRemembered()
        testCanSave()
        testSections()
        testPlaces()
        testRoots()
        reachedEnd = true
    }
}
