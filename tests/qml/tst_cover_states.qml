// SPDX-License-Identifier: LGPL-2.1-or-later
// SCR-58 per page on top. No thumbnailer, no catalogue: pictures and sizes in texts unproven.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/cover"
import "../../qml/pages"
import "../../qml/viewers"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property bool reachedEnd
    readonly property string base: "lautta://user-documents/Cover121/"
    readonly property var files: ["a.md", "b.md", "c.md", "d.md", "e.md", "f.md", "g.md", "h.md", "i.md", "j.md", "photo.jpg"]
    property var browse
    property bool stuck

    ViewerTools { id: tools }
    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: folder }

    Item {
        id: frame

        width: Theme.coverSizeLarge.width
        height: Theme.coverSizeLarge.height

        CoverPage {
            id: cover

            anchors.fill: parent
            live: false
        }
    }

    Item {
        width: Theme.coverSizeLarge.width
        height: Theme.coverSizeLarge.height

        TransferCover {
            id: tasks

            anchors.fill: parent
            activeCount: 0
            pausedCount: 0
            pendingCount: 0
            closing: false
            bytesDone: 0
            bytesTotal: 0
            rate: 0
        }
    }

    initialPage: Component {
        BrowsePage {
            id: browsePage

            Component.onCompleted: win.browse = browsePage
            deviceFree: 42000000000
            deviceTotal: 128000000000
            listModel: ListModel {
                ListElement { section: "recents"; uri: ""; name: ""; kind: "recent-strip"; icon: ""; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: ""; drive: ""; tiles: "[{\"id\":7,\"uri\":\"lautta://user-pictures/a.jpg\",\"name\":\"a.jpg\",\"kind\":\"previewed\",\"category\":\"image\",\"mimeType\":\"image/jpeg\",\"thumbnailSource\":\"\"},{\"id\":8,\"uri\":\"lautta://user-documents/b.md\",\"name\":\"b.md\",\"kind\":\"opened\",\"category\":\"text\",\"mimeType\":\"text/markdown\",\"thumbnailSource\":\"\"},{\"id\":9,\"uri\":\"lautta://user-pictures/c.jpg\",\"name\":\"c.jpg\",\"kind\":\"transferred\",\"category\":\"image\",\"mimeType\":\"image/jpeg\",\"thumbnailSource\":\"\"},{\"id\":10,\"uri\":\"lautta://user-pictures/d.jpg\",\"name\":\"d.jpg\",\"kind\":\"previewed\",\"category\":\"image\",\"mimeType\":\"image/jpeg\",\"thumbnailSource\":\"\"},{\"id\":11,\"uri\":\"lautta://user-pictures/e.jpg\",\"name\":\"e.jpg\",\"kind\":\"previewed\",\"category\":\"image\",\"mimeType\":\"image/jpeg\",\"thumbnailSource\":\"\"}]" }
                ListElement { section: "favourites"; uri: "lautta://user-documents/Uni"; name: "Thesis"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "1"; place: "Documents › Uni"; provider: ""; host: ""; free: -1; total: -1; fs: ""; drive: ""; tiles: "" }
                ListElement { section: "device"; uri: "lautta://user-documents/"; name: "Documents"; kind: "folder"; icon: "file-folder"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "user-documents"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: ""; drive: ""; tiles: "" }
                ListElement { section: "volumes"; uri: "lautta://vol-USB/"; name: "USB stick"; kind: "volume"; icon: "image://theme/icon-m-usb"; status: "locked"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "vol-USB"; place: ""; provider: ""; host: ""; free: -1; total: 64000000000; fs: ""; drive: ""; tiles: "" }
                ListElement { section: "volumes"; uri: "lautta://vol-Odd/"; name: "Sizeless"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "vol-Odd"; place: ""; provider: ""; host: ""; free: 0; total: 0; fs: "exFAT"; drive: ""; tiles: "" }
                ListElement { section: "volumes"; uri: "lautta://vol-SD/"; name: "SD card"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "vol-SD"; place: ""; provider: ""; host: ""; free: 32000000000; total: 128000000000; fs: "exFAT"; drive: ""; tiles: "" }
                ListElement { section: "volumes"; uri: "lautta://vol-SD2/"; name: "Second card"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "vol-SD2"; place: ""; provider: ""; host: ""; free: 1000; total: 2000; fs: "exFAT"; drive: ""; tiles: "" }
            }
        }
    }

    Component { id: folderPage; DirectoryPage {} }
    Component { id: plainPage; Page {} }
    Component {
        id: slowPage

        Page {
            readonly property string coverKind: "grid"
            readonly property string coverTitle: "Slow"
            readonly property var coverModel: ListModel { property bool loading: true }
        }
    }
    Component { id: trashPage; TrashPage {} }
    Component { id: recentsPage; RecentsPage {} }
    Component { id: imageViewer; ImageViewer {} }
    Component { id: markdownViewer; MarkdownViewer {} }
    Component { id: mediaPlayer; MediaPlayer {} }
    Component { id: pdfViewer; PdfViewer {} }
    Component { id: textViewer; TextViewer {} }

    ListModel {
        id: recentRows

        ListElement { itemId: 1; uri: "lautta://user-documents/Cover121/a.md"; name: "a.md"; kind: "opened"; place: "Documents"; at: 0; dateGroup: "today" }
        ListElement { itemId: 2; uri: "lautta://user-documents/Cover121/photo.jpg"; name: "photo.jpg"; kind: "previewed"; place: "Documents"; at: 0; dateGroup: "today" }
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    // Condition polled; bound only guards a hang. One hang ends all waiting.
    function waitFor(done, what) {
        for (var i = 0; i < 400 && !stuck && !done(); ++i)
            tools.pump(50)
        stuck = stuck || !done()
        return expect(!stuck, "timed out waiting for " + what)
    }

    function findAll(item, pred, out) {
        if (!item)
            return out
        if (pred(item) && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    function shown(item) {
        for (var x = item; x; x = x.parent) {
            if (!x.visible)
                return false
        }
        return true
    }

    // Named items on screen, in reading order.
    function named(root, name) {
        var views = findAll(root, function (x) { return x.forceLayout !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
        return findAll(root, function (x) { return x.objectName === name && shown(x) }, []).sort(function (a, b) {
            var pa = a.mapToItem(root, 0, 0)
            var pb = b.mapToItem(root, 0, 0)
            return pa.y !== pb.y ? pa.y - pb.y : pa.x - pb.x
        })
    }

    function texts(root, name) {
        return named(root, name).map(function (x) { return x.text }).join("|")
    }

    function at(item) {
        var p = item.mapToItem(frame, 0, 0)
        return Math.round(p.x) + "," + Math.round(p.y)
    }

    function push(component, props) {
        var page = pageStack.push(component, props, PageStackAction.Immediate)
        waitFor(function () { return pageStack.currentPage === page && !pageStack.busy }, "page on top")
        return page
    }

    function popTo(depth) {
        while (pageStack.depth > depth) {
            var want = pageStack.depth - 1
            pageStack.pop(null, PageStackAction.Immediate)
            waitFor(function () { return pageStack.depth === want && !pageStack.busy }, "page gone")
        }
    }

    function makeFolder() {
        documents.createFolder("Cover121")
        if (!waitFor(function () { return documents.indexOf(base.replace(/\/$/, "")) >= 0 }, "the folder"))
            return false
        folder.uri = base
        folder.createFolder("Empty")
        folder.createFolder("Other")
        if (!waitFor(function () { return folder.count === 2 }, "its folders"))
            return false
        for (var i = 0; i < files.length; ++i) {
            var from = files[i] === "photo.jpg" ? "host/fixtures/photo.jpg" : "host/fixtures/doc.md"
            if (!expect(tools.installFixture(Qt.resolvedUrl(from), base + files[i]), "fixture " + files[i]))
                return false
        }
        return true
    }

    function checkAsleep() {
        expect(cover.kind === "browse", "Browse on top: its cover, not " + cover.kind)
        expect(named(frame, "coverGrid").length === 0 && named(frame, "storageLine").length === 0,
               "nothing is built while the cover is out of sight")
        expect(texts(frame, "coverHeader") === "", "nor a header: " + texts(frame, "coverHeader"))
    }

    function checkBrowse() {
        expect(texts(frame, "coverHeader") === "lautta-app-name", "Browse's cover is headed Lautta: " + texts(frame, "coverHeader"))
        expect(texts(frame, "storageName") === "lautta-cover-phone|SD card",
               "phone and the first mounted volume have a line each: " + texts(frame, "storageName"))
        expect(texts(frame, "storageFree") === "lautta-cover-free|lautta-cover-free", "each tells its free space: " + texts(frame, "storageFree"))
        expect(named(frame, "storageFree").map(function (x) { return x.amount }).join("|") === "39 GB|30 GB",
               "in whole numbers while there is plenty: " + named(frame, "storageFree").map(function (x) { return x.amount }).join("|"))
        var lines = named(frame, "storageLine")
        var used = named(frame, "storageUsed")
        var width = frame.width - 2 * Theme.paddingMedium
        if (!expect(lines.length === 2 && used.length === 2, "two lines with a bar each"))
            return
        expect(lines[0].width === width && width > 0, "a line spans the cover between its margins: " + lines[0].width)
        expect(used[0].width === Math.round(width * 86 / 128) && used[1].width === Math.round(width * 96 / 128),
               "a bar fills by the space used: " + used[0].width + ", " + used[1].width + " of " + width)
        var name = named(lines[0], "storageName")[0]
        var free = named(lines[0], "storageFree")[0]
        expect(name.mapToItem(frame, 0, 0).x + name.width <= free.mapToItem(frame, 0, 0).x
               && Math.round(free.mapToItem(frame, 0, 0).x + free.width) === Math.round(frame.width - Theme.paddingMedium),
               "free space ends at the right margin, the name before it")

        var grid = named(frame, "coverGrid")[0]
        var tiles = named(frame, "coverTile")
        if (!expect(grid !== undefined && tiles.length === 5, "the newest recents are tiles: " + tiles.length))
            return
        expect(Math.round(grid.mapToItem(frame, 0, 0).y) >= Math.round(lines[1].mapToItem(frame, 0, lines[1].height).y + Theme.paddingMedium),
               "tiles start below the lines")
        var side = Math.floor((width - 2 * Theme.paddingSmall) / 3)
        expect(tiles.every(function (t) { return t.width === side && t.height === side }) && side > 0,
               "three tiles fill the cover between its margins: " + tiles[0].width + ", not " + side)
        var x = Math.round(Theme.paddingMedium)
        var y = Math.round(grid.mapToItem(frame, 0, 0).y)
        var step = side + Math.round(Theme.paddingSmall)
        expect(tiles.map(at).join(" ") === [x + "," + y, (x + step) + "," + y, (x + 2 * step) + "," + y,
                                            x + "," + (y + step), (x + step) + "," + (y + step)].join(" "),
               "three to a row, rows apart as columns are: " + tiles.map(at).join(" "))
        expect(tiles.map(function (t) { return t.category }).join(",") === "image,text,image,image,image",
               "a tile shows its file's type: " + tiles.map(function (t) { return t.category }).join(","))
        expect(named(frame, "placeGlyph").length === 0 && named(frame, "fileGlyph").length === 5, "recents draw file glyphs")
    }

    function checkPlaces() {
        browse.listModel.setProperty(0, "tiles", "")
        var tiles = named(frame, "coverTile")
        expect(tiles.length === 6 && tiles.map(function (t) { return t.placeIcon }).join(",")
               === "image://theme/icon-m-favorite,file-folder,image://theme/icon-m-usb,image://theme/icon-m-sd-card,image://theme/icon-m-sd-card,image://theme/icon-m-sd-card",
               "without recents Browse's places are the tiles: " + tiles.map(function (t) { return t.placeIcon }).join(","))
        expect(named(frame, "placeGlyph").length === 6 && named(frame, "fileGlyph").length === 0, "each draws its place's icon only")
        browse.deviceTotal = -1
        expect(texts(frame, "storageName") === "SD card", "unknown phone space has no line: " + texts(frame, "storageName"))
        browse.listModel.remove(5)
        expect(texts(frame, "storageName") === "Second card" && named(frame, "coverTile").length === 5,
               "an ejected volume leaves the cover: " + texts(frame, "storageName"))
        browse.deviceTotal = 128000000000
    }

    function checkFolder() {
        var page = push(folderPage, { "uri": base })
        var total = files.length + 2
        if (!waitFor(function () { return page.coverModel.count === total && named(frame, "coverTile").length >= 9 }, "the folder's tiles"))
            return
        expect(cover.kind === "grid" && texts(frame, "coverHeader") === "Cover121", "a folder's cover is headed by its name: " + texts(frame, "coverHeader"))
        expect(named(frame, "storageLine").length === 0, "Browse's lines are gone")
        var tiles = named(frame, "coverTile")
        expect(tiles.slice(0, 3).map(function (t) { return t.isDir + ":" + t.category }).join(",") === "true:folder,true:folder,false:markdown",
               "tiles follow the folder's order, folders first: "
               + tiles.slice(0, 3).map(function (t) { return t.isDir + ":" + t.category }).join(","))
        expect(tiles.every(function (t) { return t.placeIcon === "" }) && named(frame, "placeGlyph").length === 0
               && named(frame, "fileGlyph").length >= 9,
               "a folder's tiles draw file glyphs, none a place's")
        expect(named(frame, "fileGlyph").every(function (g) { return !g.highlighted }),
               "folders in the same plain colour as files")
        var header = named(frame, "coverHeader")[0]
        expect(Math.round(tiles[0].mapToItem(frame, 0, 0).y) === Math.round(header.mapToItem(frame, 0, header.height).y + Theme.paddingSmall),
               "tiles start under the header")
        expect(named(frame, "coverEmpty").length === 0, "no empty text beside tiles")
        frame.width = header.implicitWidth + 4 * Theme.paddingMedium
        expect(header.implicitWidth < header.width && header.horizontalAlignment === Text.AlignRight, "a name that fits stands at the right")
        frame.width = Theme.paddingMedium * 4
        expect(header.implicitWidth > header.width && header.horizontalAlignment === Text.AlignLeft, "a long one is cut at its end, not its start")
        frame.width = Theme.coverSizeLarge.width

        var empty = push(folderPage, { "uri": base + "Empty/" })
        if (!waitFor(function () { return empty.coverModel.loading === false && texts(frame, "coverHeader") === "Empty" }, "the empty folder"))
            return
        expect(named(frame, "coverTile").length === 0 && texts(frame, "coverEmpty") === "lautta-dir-empty",
               "an empty folder says so: " + texts(frame, "coverEmpty"))
        var text = named(frame, "coverEmpty")[0]
        expect(Math.abs(text.mapToItem(frame, text.width / 2, text.height / 2).y - frame.height / 2) <= 1, "in the cover's middle")

        var slow = push(slowPage, {})
        expect(texts(frame, "coverHeader") === "Slow" && named(frame, "coverEmpty").length === 0, "a folder still loading is not called empty")
        slow.coverModel.loading = false
        expect(texts(frame, "coverEmpty") === "lautta-dir-empty", "once loaded it is")
        popTo(3)

        push(plainPage, {})
        expect(cover.kind === "grid" && texts(frame, "coverHeader") === "Empty",
               "a page that tells no cover leaves the one of the page below: " + cover.kind + " " + texts(frame, "coverHeader"))
        popTo(2)
        expect(texts(frame, "coverHeader") === "Cover121", "back in the folder its cover is back: " + texts(frame, "coverHeader"))
    }

    function checkFile() {
        var viewers = [[markdownViewer, "a.md", "markdown"], [textViewer, "b.md", "markdown"], [imageViewer, "photo.jpg", "image"],
                       [pdfViewer, "c.md", "markdown"], [mediaPlayer, "d.md", "markdown"]]
        for (var i = 0; i < viewers.length; ++i) {
            push(viewers[i][0], { "uri": base + viewers[i][1] })
            var picture = named(frame, "coverPicture")
            expect(cover.kind === "file" && texts(frame, "coverHeader") === viewers[i][1] && picture.length === 1
                   && picture[0].category === viewers[i][2],
                   "viewer " + i + " shows its file: " + cover.kind + " " + texts(frame, "coverHeader"))
            popTo(2)
        }
        push(imageViewer, { "uri": base + "photo.jpg" })
        var file = findAll(frame, function (x) { return x.pictured !== undefined && x.tile !== undefined }, [])[0]
        var picture = named(frame, "coverPicture")[0]
        var header = named(frame, "coverHeader")[0]
        expect(picture.width === frame.width && picture.height === frame.height, "the file fills the cover")
        expect(picture.thumbnailSource.indexOf("file://") === 0 && picture.thumbnailSource.indexOf("photo.jpg") > 0,
               "a photo's cover asks for its picture: " + picture.thumbnailSource)
        expect(named(frame, "coverTile").length === 0, "no tiles behind it")
        expect(!file.pictured && named(frame, "coverFade").length === 0 && Qt.colorEqual(header.color, Theme.highlightColor)
               && picture.opacity < 1,
               "without a picture: a dimmed glyph, no fade, name in highlight colour")
        file.pictured = true
        var fade = named(frame, "coverFade")
        expect(fade.length === 1 && fade[0].width === frame.width
               && fade[0].height > header.mapToItem(frame, 0, header.height).y
               && Qt.colorEqual(header.color, Theme.lightPrimaryColor) && picture.opacity === 1,
               "on a picture: name in light colour on a dark fade that reaches below it")
        popTo(2)
    }

    function checkLists() {
        push(trashPage, {})
        expect(cover.kind === "grid" && texts(frame, "coverHeader") === "lautta-trash-title" && named(frame, "coverTile").length === 0
               && texts(frame, "coverEmpty") === "lautta-dir-empty",
               "an empty Trash has its title and says No files: " + texts(frame, "coverHeader") + " " + texts(frame, "coverEmpty"))
        popTo(2)
        push(recentsPage, { "listModel": recentRows })
        var tiles = named(frame, "coverTile")
        expect(texts(frame, "coverHeader") === "lautta-recents-title" && tiles.length === 2
               && tiles.map(function (t) { return t.category }).join(",") === "markdown,image",
               "Recents has its title and a tile per file, by type: " + texts(frame, "coverHeader") + " "
               + tiles.map(function (t) { return t.category }).join(","))
        expect(named(frame, "coverEmpty").length === 0, "no empty text beside them")
        expect(tiles[0].thumbnailSource === "" && tiles[1].thumbnailSource.indexOf("file://") === 0
               && tiles[1].thumbnailSource.indexOf("photo.jpg") > 0,
               "a recent photo's tile asks for its picture: " + tiles[1].thumbnailSource)
        popTo(1)
        expect(cover.kind === "browse" && texts(frame, "coverHeader") === "lautta-app-name", "back on Browse its cover is back")
    }

    function checkTaskCover() {
        var root = tasks.parent
        expect(tasks.mode === "idle" && !tasks.active, "no task: idle")
        tasks.activeCount = 3
        tasks.bytesTotal = 1000
        var fill = named(root, "coverFill")
        var progress = named(root, "coverProgress")
        if (!expect(tasks.mode === "transferring" && fill.length === 1 && progress.length === 1, "running tasks show fill and percent"))
            return
        expect(texts(root, "coverHeader") === "lautta-xfr-cover-count", "header counts the tasks: " + texts(root, "coverHeader"))
        var widths = [100, 500, 900, 2000].map(function (done) {
            tasks.bytesDone = done
            return fill[0].width
        }).join(",")
        expect(widths === [0.1, 0.5, 0.9, 1].map(function (f) { return Math.round(root.width * f) }).join(","),
               "the fill grows from the left with the bytes done, never past the cover: " + widths)
        expect(fill[0].x === 0 && fill[0].y === 0 && fill[0].height === root.height, "it is as tall as the cover")
        tasks.bytesDone = 500
        expect(findAll(progress[0], function (x) { return x.text === "50%" }, []).length === 1, "percent is told")
        expect(Math.abs(progress[0].y + progress[0].height / 2 - root.height / 2) <= 1, "in the cover's middle")
        expect(named(root, "coverRate").length === 0, "no rate while none is known")
        tasks.rate = 2048
        expect(texts(root, "coverRate") === "lautta-xfr-rate", "a known rate is told: " + texts(root, "coverRate"))
        expect(Math.abs(progress[0].y + progress[0].height / 2 - root.height / 2) <= 1, "both lines stay around the middle")
        var running = String(fill[0].color)
        expect(fill[0].color.a > 0 && fill[0].color.a < 0.5, "the fill is see-through: " + fill[0].color.a)

        tasks.pausedCount = 2
        expect(tasks.mode === "transferring", "one task running beside paused ones: still running")
        tasks.activeCount = 0
        expect(tasks.mode === "paused" && texts(root, "coverHeader") === "lautta-xfr-header-paused",
               "all paused: header says so: " + texts(root, "coverHeader"))
        expect(fill[0].visible && String(fill[0].color) !== running && fill[0].color.a > 0 && fill[0].color.a < 0.5
               && fill[0].width === Math.round(root.width / 2),
               "a paused fill keeps its width and loses the highlight colour")

        tasks.pendingCount = 2
        expect(tasks.mode === "paused", "tasks wait only once the app closes: " + tasks.mode)
        tasks.closing = true
        expect(tasks.mode === "pending" && named(root, "coverFill").length === 0 && named(root, "coverProgress").length === 0
               && texts(root, "coverHeader") === "lautta-app-name",
               "tasks waiting at close: no fill, no percent, header Lautta: " + texts(root, "coverHeader"))
        tasks.closing = false
        tasks.pausedCount = 0
        tasks.pendingCount = 0
        expect(!tasks.active, "all done: idle again")
    }

    function checkTasksWin() {
        cover.tasks.activeCount = 1
        expect(cover.kind === "tasks" && named(frame, "coverGrid").length === 0 && named(frame, "storageLine").length === 0,
               "a running task takes the cover: " + cover.kind)
        expect(texts(frame, "coverHeader") === "lautta-xfr-cover-count", "one header only, the tasks': " + texts(frame, "coverHeader"))
        cover.tasks.activeCount = 0
        expect(cover.kind === "browse" && named(frame, "coverGrid").length === 1, "the page's cover is back once it ends")
    }

    Timer {
        interval: 1
        running: win.browse !== undefined && win.browse.status === PageStatus.Active && !pageStack.busy
        onTriggered: {
            if (!win.makeFolder()) {
                win.scenarioRunning = false
                return
            }
            win.checkAsleep()
            cover.live = true
            win.checkBrowse()
            win.checkTasksWin()
            win.checkPlaces()
            win.checkFolder()
            win.checkFile()
            win.checkLists()
            win.checkTaskCover()
            win.reachedEnd = true
            win.scenarioRunning = false
        }
    }
}
