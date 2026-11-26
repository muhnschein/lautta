// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/cover"
import "../../qml/pages"
import "../../qml/viewers"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property bool scenarioStarted
    property bool reachedEnd
    readonly property string base: "lautta://user-documents/Cover121/"
    readonly property var files: ["a.md", "b.md", "c.md", "d.md", "e.md", "f.md", "g.md", "h.md", "i.md", "j.md", "photo.jpg"]
    property var home
    property bool stuck

    ViewerTools { id: tools }
    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: folder }

    Item {
        id: frame

        width: 10 * Theme.paddingLarge
        height: 16 * Theme.paddingLarge

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
        HomePage {
            id: homePage

            Component.onCompleted: win.home = homePage
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
    Component { id: plainDialog; Dialog {} }
    Component { id: lateCover; CoverPage { live: false } }
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
        expect(cover.kind === "home", "Home on top: its cover, not " + cover.kind)
        expect(named(frame, "coverGrid").length === 0 && named(frame, "coverLanes").length === 0,
               "nothing is built while the cover is out of sight")
        expect(texts(frame, "coverHeader") === "", "nor a header: " + texts(frame, "coverHeader"))
    }

    function listOf(items) {
        var out = []
        for (var i = 0; i < items.length; ++i)
            out.push(items[i])
        return out
    }

    function pose(lanes) {
        return [lanes.opacity, lanes.x, lanes.y].concat(listOf(lanes.children).map(function (l) {
            return [l.x, l.y, l.opacity, l.scale, l.rotation].join(":")
        })).join(" ")
    }

    function checkHome() {
        expect(named(frame, "coverHeader").length === 0, "Home's cover has no header")
        var lanes = named(frame, "coverLanes")
        if (!expect(lanes.length === 1, "Home's cover is the lanes: " + lanes.length))
            return
        expect(lanes[0].width === frame.width && lanes[0].height === frame.height && lanes[0].clip,
               "they fill the cover and stay inside it: " + lanes[0].width + "x" + lanes[0].height)
        expect(named(frame, "coverTile").length === 0 && named(frame, "fileGlyph").length === 0
               && named(frame, "textPreview").length === 0 && named(frame, "coverGrid").length === 0,
               "no recents, files or previews on it, though Home has recents")
        var faint = named(lanes[0], "lanesFaint")
        var lit = named(lanes[0], "lanesLit")
        if (!expect(faint.length === 1 && lit.length === 1, "two drawn layers: " + faint.length + " " + lit.length))
            return
        expect(String(faint[0].source).match(/cover\/lanes-faint\.svg$/) !== null && String(lit[0].source).match(/cover\/lanes-lit\.svg$/) !== null,
               "the prerendered lanes: " + faint[0].source + " " + lit[0].source)
        expect(String(faint[0].color) === String(Theme.primaryColor) && String(lit[0].color) === String(Theme.highlightColor),
               "faint lanes in primary, lit lanes in the ambience's highlight: " + faint[0].color + " " + lit[0].color)
        expect([faint[0], lit[0]].every(function (i) {
                   return i.width === frame.width && i.height === frame.height && i.x === 0 && i.y === 0
                       && i.sourceSize.width === Math.round(frame.width) && i.sourceSize.height === Math.round(frame.height)
                       && i.fillMode === Image.PreserveAspectCrop && i.status === Image.Ready && i.cache
               }), "both fill the cover, drawn at its size, ready, cached: " + lit[0].sourceSize + " " + lit[0].status)
        var layers = listOf(lanes[0].children)
        expect(lit[0].z >= faint[0].z && layers.indexOf(lit[0]) > layers.indexOf(faint[0]), "lit lanes over faint ones")
        cover.live = false
        expect(named(frame, "coverLanes").length === 0, "hidden cover drops the lanes")
        cover.live = true
        var shownAgain = named(frame, "coverLanes")
        if (!expect(shownAgain.length === 1, "shown again, the lanes are back"))
            return
        var still = pose(shownAgain[0])
        tools.pump(1000)
        expect(pose(shownAgain[0]) === still, "nothing moves on the cover in its first second: " + still + " then " + pose(shownAgain[0]))
    }

    function checkFolder() {
        var page = push(folderPage, { "uri": base })
        var total = files.length + 2
        if (!waitFor(function () { return page.coverModel.count === total && named(frame, "coverTile").length >= 9 }, "the folder's tiles"))
            return
        expect(cover.kind === "grid" && texts(frame, "coverHeader") === "Cover121", "a folder's cover is headed by its name: " + texts(frame, "coverHeader"))
        expect(named(frame, "coverLanes").length === 0, "Home's lanes are gone")
        var tiles = named(frame, "coverTile")
        expect(tiles.slice(0, 3).map(function (t) { return t.isDir + ":" + t.category }).join(",") === "true:folder,true:folder,false:markdown",
               "tiles follow the folder's order, folders first: "
               + tiles.slice(0, 3).map(function (t) { return t.isDir + ":" + t.category }).join(","))
        var note = tiles[2]
        if (waitFor(function () { return named(note, "textPreview").length === 1 }, "a text file's first lines on its tile")) {
            expect(named(note, "textPreview")[0].text.indexOf("# Title") === 0,
                   "the tile shows the file's start: " + JSON.stringify(named(note, "textPreview")[0].text.slice(0, 40)))
            expect(named(note, "textPreview")[0].font.family !== App.monoFont, "prose in the text font")
            expect(named(note, "fileGlyph").length === 0, "no glyph under the text")
            expect(note.textAsked === "", "the folder's own text, not read a second time: " + note.textAsked)
            expect(named(note, "typeIcon").length === 1 && named(note, "typeIcon")[0].icon === note.typeIcon && note.typeIcon !== "",
                   "its type icon in the corner: " + note.typeIcon)
        }
        expect(named(tiles[0], "textPreview").length === 0 && named(tiles[0], "fileGlyph").length === 1, "a folder keeps its glyph")
        expect(named(frame, "fileGlyph").length + named(frame, "textPreview").length >= 9,
               "a folder's tiles draw file glyphs or text")
        var wide = Math.floor((frame.width - 2 * Theme.paddingSmall) / 3)
        expect(Math.round(tiles[0].mapToItem(frame, 0, 0).x) === 0 && tiles[0].width === wide
               && Math.round(tiles[2].mapToItem(frame, 0, 0).x + tiles[2].width) >= Math.round(frame.width) - 2,
               "a folder's tiles run from edge to edge: " + tiles[0].width + ", not " + wide)
        expect(named(frame, "fileGlyph").every(function (g) { return !g.highlighted }),
               "folders in the same plain colour as files")
        var header = named(frame, "coverHeader")[0]
        expect(Math.round(header.x) === Math.round(Theme.paddingLarge) && Math.round(header.y) === Math.round(Theme.paddingLarge)
               && Math.round(header.x + header.width) === Math.round(frame.width - Theme.paddingLarge),
               "header keeps a large padding from top and both sides: " + header.x + ", " + header.y + ", " + header.width)
        expect(Math.round(tiles[0].mapToItem(frame, 0, 0).y) === Math.round(header.mapToItem(frame, 0, header.height).y + Theme.paddingLarge),
               "tiles start as far under the header as it is from the top")
        expect(named(frame, "coverEmpty").length === 0, "no empty text beside tiles")
        frame.width = header.implicitWidth + 4 * Theme.paddingLarge
        expect(header.implicitWidth < header.width && header.horizontalAlignment === Text.AlignRight, "a name that fits stands at the right")
        frame.width = Theme.paddingMedium * 4
        expect(header.implicitWidth > header.width && header.horizontalAlignment === Text.AlignLeft, "a long one is cut at its end, not its start")
        frame.width = 10 * Theme.paddingLarge

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
        if (waitFor(function () { return named(tiles[0], "textPreview").length === 1 }, "a recent text file's first lines on its tile")) {
            expect(named(tiles[0], "textPreview")[0].text.indexOf("# Title") === 0 && named(tiles[0], "textPreview")[0].font.family !== App.monoFont,
                   "the recent's tile shows the file's start in the text font: " + JSON.stringify(named(tiles[0], "textPreview")[0].text.slice(0, 40)))
            expect(named(tiles[0], "typeIcon").length === 1 && tiles[0].typeIcon !== "", "with its type icon: " + tiles[0].typeIcon)
        }
        expect(named(tiles[1], "textPreview").length === 0 && tiles[1].textAsked === "" && tiles[0].textAsked === recentRows.get(0).uri,
               "a photo's tile carries no text and asks for none: " + tiles[1].textAsked)
        expect(tiles[0].thumbnailSource === "" && tiles[1].thumbnailSource.indexOf("file://") === 0
               && tiles[1].thumbnailSource.indexOf("photo.jpg") > 0,
               "a recent photo's tile asks for its picture: " + tiles[1].thumbnailSource)
        popTo(1)
        expect(cover.kind === "home" && named(frame, "coverLanes").length === 1 && named(frame, "coverHeader").length === 0,
               "back on Home its cover is back")
    }

    function lineText(root) {
        return named(root, "coverLine").map(function (l) { return l.name }).join(",")
    }

    function checkListView() {
        App.setSetting("view_mode", JSON.stringify("list"))
        var page = push(folderPage, { "uri": base })
        var total = files.length + 2
        if (!waitFor(function () { return page.coverModel.count === total && named(frame, "coverLine").length >= 6 }, "the folder's lines"))
            return
        expect(cover.kind === "list" && texts(frame, "coverHeader") === "Cover121",
               "a folder in list view has a list cover under its name: " + cover.kind + " " + texts(frame, "coverHeader"))
        expect(named(frame, "coverTile").length === 0 && named(frame, "coverGrid").length === 0, "no tiles beside the lines")
        var lines = named(frame, "coverLine")
        expect(lineText(frame).indexOf("Empty,Other,a.md,b.md") === 0, "lines follow the folder's order, folders first: " + lineText(frame))
        var icons = lines.map(function (l) { return named(l, "coverLineIcon")[0] })
        expect(icons[0].isDir && icons[0].category === "folder" && !icons[2].isDir && icons[2].category === "markdown",
               "each line has its item's glyph: " + icons[0].category + " " + icons[2].category)
        expect(icons.every(function (i) { return i.width === Theme.iconSizeSmall && i.glyphSize === Theme.iconSizeSmall
                                                 && i.previewText === "" && Math.round(i.mapToItem(frame, 0, 0).x) === Math.round(Theme.paddingLarge) }),
               "small glyphs, no text previews, on the header's margin: " + icons[0].width + " " + at(icons[0]))
        var label = findAll(lines[2], function (x) { return x.text === "a.md" && x.truncationMode !== undefined }, [])[0]
        expect(label.font.pixelSize === Theme.fontSizeExtraSmall && label.truncationMode === TruncationMode.Fade
               && Math.round(label.mapToItem(frame, label.width, 0).x) === Math.round(frame.width)
               && Math.round(label.mapToItem(frame, 0, 0).x) === Math.round(Theme.paddingLarge + Theme.iconSizeSmall + Theme.paddingMedium),
               "a name beside its glyph fades out at the cover's edge: " + at(label) + " " + label.width)
        expect(lines[1].height === Theme.iconSizeSmall + Theme.paddingMedium
               && Math.round(lines[1].mapToItem(frame, 0, 0).y - lines[0].mapToItem(frame, 0, 0).y) === Math.round(lines[0].height),
               "lines stack without gaps: " + lines[0].height)
        var header = named(frame, "coverHeader")[0]
        expect(Math.round(lines[0].mapToItem(frame, 0, 0).y) === Math.round(header.mapToItem(frame, 0, header.height).y + Theme.paddingLarge),
               "lines start where tiles would")
        var list = named(frame, "coverList")[0]
        expect(list.clip && Math.round(list.mapToItem(frame, 0, list.height).y) === Math.round(frame.height), "and stay inside the cover")

        App.setSetting("view_mode", JSON.stringify("grid"))
        if (waitFor(function () { return cover.kind === "grid" }, "the grid cover"))
            expect(named(frame, "coverLine").length === 0 && named(frame, "coverTile").length >= 9, "switched to grid, the cover shows tiles")
        App.setSetting("view_mode", JSON.stringify("list"))
        if (waitFor(function () { return cover.kind === "list" }, "the list cover"))
            expect(named(frame, "coverLine").length >= 6 && named(frame, "coverTile").length === 0, "and back to lines")
        page.coverModel.setViewPrefs(JSON.stringify({ "viewMode": "grid" }))
        if (waitFor(function () { return cover.kind === "grid" }, "the folder's own grid"))
            expect(named(frame, "coverTile").length >= 9, "a folder set to grid keeps tiles though the app shows lists")
        page.coverModel.setViewPrefs(JSON.stringify({ "viewMode": "list" }))
        waitFor(function () { return cover.kind === "list" }, "the folder back in list view")

        var empty = push(folderPage, { "uri": base + "Empty/" })
        if (waitFor(function () { return empty.coverModel.loading === false && texts(frame, "coverHeader") === "Empty" }, "the empty folder"))
            expect(cover.kind === "list" && named(frame, "coverLine").length === 0 && texts(frame, "coverEmpty") === "lautta-dir-empty",
                   "an empty folder in list view says so: " + texts(frame, "coverEmpty"))
        popTo(1)

        push(recentsPage, { "listModel": recentRows })
        expect(cover.kind === "list" && texts(frame, "coverHeader") === "lautta-recents-title" && lineText(frame) === "a.md,photo.jpg",
               "Recents in list view: a line per file: " + cover.kind + " " + lineText(frame))
        var recent = named(frame, "coverLineIcon")
        expect(recent.length === 2 && recent[0].category === "markdown" && recent[1].thumbnailSource.indexOf("photo.jpg") > 0,
               "by type, with the photo's picture")
        expect(named(frame, "coverEmpty").length === 0, "no empty text beside them")
        popTo(1)
        push(trashPage, {})
        expect(cover.kind === "list" && texts(frame, "coverHeader") === "lautta-trash-title" && named(frame, "coverLine").length === 0
               && texts(frame, "coverEmpty") === "lautta-dir-empty",
               "an empty Trash in list view says No files: " + cover.kind + " " + texts(frame, "coverEmpty"))
        popTo(1)
        App.setSetting("view_mode", JSON.stringify("grid"))
    }

    function checkDialogClosing() {
        var below = pageStack.currentPage
        var dialog = pageStack.push(plainDialog)
        if (!waitFor(function () { return pageStack.currentPage === dialog && !pageStack.busy }, "the dialog to open"))
            return
        dialog.accept()
        if (!expect(pageStack.busy && pageStack.currentPage === dialog
                    && pageStack.find(function (page) { return page === dialog }) === null,
                    "the closing dialog is still the current page, but out of the stack"))
            return
        var late = lateCover.createObject(frame)
        expect(late.source === below && late.kind === "home",
               "a cover made while a dialog closes shows the page below it: " + late.source + " " + late.kind)
        late.destroy()
        waitFor(function () { return pageStack.currentPage === below && !pageStack.busy }, "the dialog to close")
        expect(cover.kind === "home", "the cover is Home's once it closed: " + cover.kind)
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
        expect(cover.kind === "tasks" && named(frame, "coverGrid").length === 0 && named(frame, "coverLanes").length === 0,
               "a running task takes the cover: " + cover.kind)
        expect(texts(frame, "coverHeader") === "lautta-xfr-cover-count", "one header only, the tasks': " + texts(frame, "coverHeader"))
        cover.tasks.activeCount = 0
        expect(cover.kind === "home" && named(frame, "coverLanes").length === 1, "the page's cover is back once it ends")
    }

    Timer {
        interval: 1
        running: !win.scenarioStarted && win.home !== undefined && win.home.status === PageStatus.Active && !pageStack.busy
        onTriggered: {
            win.scenarioStarted = true
            if (!win.makeFolder()) {
                win.scenarioRunning = false
                return
            }
            App.setSetting("view_mode", JSON.stringify("grid"))
            win.checkAsleep()
            cover.live = true
            win.checkHome()
            win.checkTasksWin()
            win.checkFolder()
            win.checkFile()
            win.checkLists()
            win.checkListView()
            win.checkDialogClosing()
            win.checkTaskCover()
            win.reachedEnd = true
            win.scenarioRunning = false
        }
    }
}
