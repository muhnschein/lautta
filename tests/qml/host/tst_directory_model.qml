// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of DirectoryModel, PathModel and PickerRootsModel (no Silica):
// listing a temp HOME's Documents, create/rename errors, sort, filter, hidden
// files, selection, live updates, per-folder view settings and the path menu.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property var events: []
    readonly property string docs: "lautta://user-documents/"

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function names(rep) {
        var out = []
        for (var i = 0; i < rep.count; ++i)
            out.push(rep.itemAt(i).nm)
        return out
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function row(rep, name) {
        for (var i = 0; i < rep.count; ++i)
            if (rep.itemAt(i).nm === name)
                return rep.itemAt(i)
        return null
    }

    function wait(ms) { pump.pump(ms) }

    PickerRootsModel { id: pump }

    DirectoryModel {
        id: dir
        uri: root.docs
        onCreated: root.events.push("created:" + uri + ":" + isDir)
        onCreateFailed: root.events.push("createFailed:" + kind)
        onRenamed: root.events.push("renamed:" + oldUri + ">" + newUri)
        onRenameFailed: root.events.push("renameFailed:" + kind)
    }
    Repeater {
        id: rep
        model: dir
        delegate: Item {
            property string nm: name
            property bool dirRow: isDir
            property string thumb: thumbnailSource
            property bool sel: selected
            property string cat: category
        }
    }

    DirectoryModel { id: other; uri: root.docs }
    DirectoryModel { id: missing; uri: "lautta://user-documents/nonexistent" }
    DirectoryModel { id: broken; uri: "not a uri" }
    PathModel { id: path; uri: root.docs + "Zeta" }
    PathModel { id: rootPath; uri: root.docs }

    function testBasics() {
        wait(300)
        check(!dir.loading, "listing finished")
        check(dir.count === 0 && rep.count === 0, "empty Documents")
        check(dir.errorKind === "", "no error")
        check(dir.title === "Documents" && dir.locationName === "Documents", "title at the root is the location: " + dir.title)
        check(dir.writable && dir.capabilities.indexOf("Write") >= 0, "writable local folder")
        check(dir.hasCapability("Write") && !dir.hasCapability("Nope"), "hasCapability")
        check(dir.viewMode === "list" && dir.sortKey === "name" && dir.foldersFirst, "default view settings")
        check(!dir.offline && !dir.stale && !dir.large, "plain state flags")
    }

    function testCreate() {
        dir.createFolder("Zeta")
        dir.createFile("alpha.txt")
        dir.createFile("Beta.png")
        dir.createFile("notes.md")
        dir.createFile(".hidden")
        wait(500)
        same(names(rep), ["Zeta", "alpha.txt", "Beta.png", "notes.md"], "folders first, hidden files off")
        check(root.events.indexOf("created:" + root.docs + "Zeta:true") >= 0, "created signal for the folder")
        check(root.events.indexOf("created:" + root.docs + ".hidden:false") >= 0, "created signal for the hidden file")
        var z = row(rep, "Zeta")
        check(z.dirRow && z.cat === "folder", "folder row")
        check(row(rep, "alpha.txt").cat === "text", "category of a text file")
        check(row(rep, "Beta.png").thumb.indexOf("file://") === 0, "local images get a thumbnail source: " + row(rep, "Beta.png").thumb)
        check(row(rep, "notes.md").thumb === "", "no thumbnail for other files")

        dir.createFolder("Zeta")
        dir.createFile("alpha.txt")
        dir.createFile("bad/name")
        wait(300)
        check(root.events.filter(function (e) { return e === "createFailed:AlreadyExists" }).length === 2, "name clashes are reported: " + root.events)
        check(root.events.indexOf("createFailed:InvalidName") >= 0, "invalid names are reported")
        check(dir.nameKind("Zeta") === "folder" && dir.nameKind("alpha.txt") === "file" && dir.nameKind("zzz") === "", "nameKind")
    }

    function testSortFilterHidden() {
        check(dir.setViewPrefs(JSON.stringify({ descending: true, foldersFirst: false })), "setViewPrefs accepted")
        same(names(rep), ["Zeta", "notes.md", "Beta.png", "alpha.txt"], "descending, folders mixed")
        check(dir.descending && !dir.foldersFirst, "properties follow")
        check(dir.setViewPrefs(JSON.stringify({ descending: false, foldersFirst: true, showHidden: true })), "second setViewPrefs")
        same(names(rep), ["Zeta", ".hidden", "alpha.txt", "Beta.png", "notes.md"], "hidden files shown")
        check(dir.showHidden, "showHidden property")
        check(!dir.setViewPrefs("nonsense"), "bad JSON refused")
        dir.setViewPrefs(JSON.stringify({ showHidden: false }))
        dir.filterText = "ALPHA"
        same(names(rep), ["alpha.txt"], "filter ignores case")
        dir.filterText = ""
        dir.foldersOnly = true
        same(names(rep), ["Zeta"], "folders only")
        dir.foldersOnly = false
        check(dir.count === 4, "files back")
        check(dir.setViewPrefs(JSON.stringify({ sortKey: "type" })) && dir.sortKey === "type", "sort key")
        dir.setViewPrefs(JSON.stringify({ sortKey: "name" }))
    }

    function testSelection() {
        var a = root.docs + "alpha.txt"
        dir.toggle(a)
        check(dir.selectedCount === 1 && row(rep, "alpha.txt").sel, "toggle selects")
        same(dir.selectedUris(), [a], "selectedUris")
        dir.select(root.docs + "notes.md")
        check(dir.selectedCount === 2, "select adds")
        dir.select("lautta://user-downloads/x")
        dir.toggle("garbage")
        check(dir.selectedCount === 2, "foreign and bad URIs are ignored")
        dir.toggle(a)
        check(dir.selectedCount === 1 && !row(rep, "alpha.txt").sel, "toggle deselects")
        dir.selectAll()
        check(dir.selectedCount === 4 && dir.selectedUris().length === 4, "select all")
        dir.clearSelection()
        check(dir.selectedCount === 0 && dir.selectedUris().length === 0, "clear")
        check(dir.indexOf(a) === 1 && dir.indexOf(root.docs + "none") === -1, "indexOf: " + dir.indexOf(a))
    }

    function testRename() {
        var a = root.docs + "alpha.txt"
        dir.rename(a, "gamma.txt")
        wait(400)
        check(root.events.indexOf("renamed:" + a + ">" + root.docs + "gamma.txt") >= 0, "renamed signal: " + root.events)
        same(names(rep), ["Zeta", "Beta.png", "gamma.txt", "notes.md"], "list follows the rename")
        dir.rename(root.docs + "gamma.txt", "notes.md")
        dir.rename(root.docs + "gamma.txt", "a/b")
        wait(300)
        check(root.events.indexOf("renameFailed:AlreadyExists") >= 0, "rename onto an existing name fails")
        check(root.events.indexOf("renameFailed:InvalidName") >= 0, "invalid new name fails")
    }

    function testLiveUpdate() {
        other.createFile("watched.txt")
        wait(1200)
        check(names(rep).indexOf("watched.txt") >= 0, "changes by others appear without refresh: " + names(rep))
        other.createFile("watched2.txt")
        wait(1200)
        check(dir.count === 6, "second live update")
    }

    function testStates() {
        wait(200)
        check(missing.errorKind === "NotFound" && missing.count === 0 && !missing.loading, "missing folder: " + missing.errorKind)
        check(broken.errorKind === "InvalidArgument" && !broken.loading, "bad URI")
        var zeta = root.docs + "Zeta"
        check(path.count === 2 && path.address === "~/Documents/Zeta", "path model: " + path.count + " " + path.address)
        check(rootPath.count === 1, "root path has one step")
        check(path.breadcrumb === "Documents" && path.fullPath === "Documents › Zeta", "breadcrumbs: " + path.breadcrumb + " | " + path.fullPath)
        check(rootPath.breadcrumb === "" && rootPath.fullPath === "Documents", "root breadcrumbs")
        var found = path.complete("~/Documents/z")
        same(found, [zeta], "completion from the cached listing")
        check(path.resolve("~/Documents/Zeta") === zeta, "resolve")
        check(path.resolve("nowhere") === "", "resolve refuses")
    }

    function testPrefsPersist() {
        dir.setViewPrefs(JSON.stringify({ viewMode: "grid", thumbnails: false }))
        var again = Qt.createQmlObject('import Lautta 1.0; DirectoryModel { uri: "' + root.docs + '" }', root)
        check(again.viewMode === "grid" && !again.thumbnails, "folder view settings are stored")
        check(row(rep, "Beta.png").thumb === "", "thumbnail role follows the setting")
        check(dir.setViewPrefs(JSON.stringify({ scope: "all" })), "forget the folder's own settings")
        var third = Qt.createQmlObject('import Lautta 1.0; DirectoryModel { uri: "' + root.docs + '" }', root)
        check(third.viewMode === "list", "inherits again")
    }

    function testPicker() {
        check(pump.hasRemote() === false, "no servers in a plain home")
    }

    Component.onCompleted: {
        testBasics()
        testCreate()
        testSortFilterHidden()
        testSelection()
        testRename()
        testLiveUpdate()
        testStates()
        testPrefsPersist()
        testPicker()
    }
}
