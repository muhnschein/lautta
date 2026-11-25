// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import QtQml.Models 2.2
import Sailfish.Silica 1.0
import Sailfish.Share 1.0
import Lautta 1.0
import "../"
import "../transfers"
import "../viewers"
import "../Open.js" as Open
import "../ErrorText.js" as ErrorText
import "../ServerWarning.js" as ServerWarning
import "../Access.js" as Access
import "DirFormat.js" as DirFormat
import "GridRows.js" as GridRows
import "Selection.js" as Selection

Item {
    id: view

    property string uri
    property bool selectionMode
    property bool selectionWaiting
    property bool detailsShown
    property bool picking
    property string rootTitle
    property alias model: dir
    property alias leadArea: leadSlot
    property alias placesArea: placesSlot
    readonly property bool atRoot: picking && uri.length === 0
    readonly property var trail: path.trail
    readonly property bool selecting: selectionMode
    readonly property bool gridMode: dir.viewMode === "grid"
    // Paste row, Paste here and empty hint. Reads clipboardCount: canPasteInto is a call, binds to nothing.
    readonly property bool canPaste: dir.writable && App.clipboardCount > 0 && App.canPasteInto(uri)
    property string pendingName
    readonly property Item flickable: gridMode ? grid : list
    property var renamed: null
    property bool renamingDir
    property string undoingUri
    property Item moreActionsPage
    property var toMove: []
    property var made: []
    readonly property real coveredHeight: panelSpace.height
    readonly property real backTapWidth: Theme.itemSizeSmall

    signal openFolder(string uri)
    signal ancestorPicked(string uri)
    signal homePicked
    signal flashed(string itemUri)

    function setSelection(state) {
        selectionMode = state.selecting
        selectionWaiting = state.waiting
    }

    function startSelection() {
        setSelection(Selection.start(dir.selectedCount))
    }

    function endSelection() {
        setSelection({ "selecting": false, "waiting": false })
        dir.clearSelection()
    }

    function itemPressed(itemUri, isDir, mimeType) {
        if (!selecting && !isDir)
            prefetch.start(itemUri, mimeType)
    }

    function openItem(itemUri, isDir, mimeType, tileSize) {
        if (isDir)
            openFolder(itemUri)
        else
            Open.open(pageStack, App, Operations, itemUri, false, mimeType, uri, tileSize)
    }

    function offered(isDir) {
        return !picking || isDir
    }

    function newFolder(moving) {
        var dialog = openDialog("../../dialogs/NewItemDialog.qml", { "parentUri": uri })
        dialog.accepted.connect(function () {
            view.pendingName = dialog.itemName
            view.toMove = moving || []
            dir.createFolder(dialog.itemName)
        })
    }

    function add() {
        var dialog = openDialog("../../dialogs/AddDialog.qml", { "parentUri": uri })
        dialog.picked.connect(function (choice, next) {
            if (choice === "scan") {
                next.owner = view
                next.scan = scanMaker.createObject(view)
                return
            }
            next.accepted.connect(function () {
                view.pendingName = next.itemName
                view.toMove = []
                dir.createFolder(next.itemName)
            })
        })
    }

    function endScan(scan, savedUri) {
        scan.destroy()
        if (savedUri)
            landed([savedUri], true)
    }

    function rename(info) {
        var dialog = openDialog("../../dialogs/RenameDialog.qml", { "uri": info.uri, "isDir": info.isDir })
        dialog.accepted.connect(function () {
            view.pendingName = dialog.newName
            view.renamingDir = info.isDir === true
            dir.rename(info.uri, dialog.newName)
        })
    }

    function renameDone(oldUri, newUri) {
        if (oldUri === undoingUri) {
            undoingUri = ""
            return
        }
        var ms = App.setting("remorse_seconds") * 1000
        renamed = { "uri": newUri, "oldUri": oldUri, "isDir": renamingDir, "ends": Date.now() + ms, "revealed": false }
        renamedTimer.interval = ms
        renamedTimer.restart()
    }

    function showRenameRemorse(row) {
        var r = renamed
        var left = r ? r.ends - Date.now() : 0
        if (left <= 0)
            return null
        var shown = { "rename": r, "orphaned": false }
        //% "Folder renamed"
        var text = r.isDir ? qsTrId("lautta-dir-renamed-folder")
                           //% "File renamed"
                           : qsTrId("lautta-dir-renamed-file")
        var keep = function () {
            if (!shown.orphaned)
                view.forgetRename(r)
        }
        var remorse = typeof row.remorseAction === "function" ? row.remorseAction(text, keep, left)
                                                              : Remorse.itemAction(row.contentItem, text, keep, left)
        if (remorse)
            onceCanceled(remorse, function () { view.undoRename(r) })
        return shown
    }

    function onceCanceled(remorse, action) {
        var done = function () {
            remorse.canceled.disconnect(canceled)
            remorse.triggered.disconnect(done)
        }
        var canceled = function () {
            done()
            action()
        }
        remorse.canceled.connect(canceled)
        remorse.triggered.connect(done)
    }

    function forgetRename(r) {
        if (r && renamed === r) {
            renamed = null
            renamedTimer.stop()
        }
    }

    function undoRename(r) {
        forgetRename(r)
        undoingUri = r.uri
        pendingName = App.nameOf(r.oldUri)
        dir.rename(r.uri, pendingName)
    }

    function revealRenamed() {
        if (!renamed || renamed.revealed || dir.loading)
            return
        var index = dir.indexOf(renamed.uri)
        if (index < 0)
            return
        renamed.revealed = true
        showIndex(index)
        flashed(renamed.uri)
    }

    function showIndex(index) {
        var f = flickable
        var row = gridMode ? GridRows.rowOf(index, grid.columns) : index
        f.positionViewAtIndex(row, ListView.Contain)
        var covered = f.indexAt(f.width / 2, f.contentY + f.height - coveredHeight)
        if (coveredHeight > 0 && covered >= 0 && row >= covered)
            f.contentY = Math.min(f.contentY + coveredHeight, f.originY + f.contentHeight - f.height)
    }

    function landed(uris, relist) {
        var bare = function (u) { return u.replace(/\/+$/, "") }
        var here = uris.filter(function (u) { return bare(App.parentUri(u)) === bare(view.uri) })
        if (here.length === 0)
            return
        made = made.concat(here)
        if (relist)
            dir.refresh()
    }

    function revealMade() {
        if (made.length === 0 || dir.loading)
            return
        var found = made.filter(function (u) { return dir.indexOf(u) >= 0 })
        made = []
        if (found.length === 0)
            return
        var rows = found.map(function (u) { return dir.indexOf(u) })
        showIndex(Math.max.apply(null, rows))
        showIndex(Math.min.apply(null, rows))
        found.forEach(function (u) { view.flashed(u) })
    }

    function fitGridRows() {
        var wanted = gridMode ? GridRows.rowCount(dir.count, grid.columns) : 0
        if (gridRows.count > wanted)
            gridRows.remove(wanted, gridRows.count - wanted)
        var more = []
        for (var i = gridRows.count; i < wanted; ++i)
            more.push({})
        if (more.length > 0)
            gridRows.append(more)
    }

    function settleRemorse() {
        if (remorsePopup.active)
            remorsePopup.trigger()
    }

    function paste() {
        settleRemorse()
        if (App.clipboardCut) {
            pasteCut()
            return
        }
        var taken = JSON.parse(App.takeClipboard())
        if (taken)
            Operations.copyTo(JSON.stringify(taken.items), uri)
    }

    function pasteCut() {
        var json = App.clipboardJson()
        var items = JSON.parse(json)
        if (items.length === 0)
            return
        var folder = uri
        var action = function () {
            if (App.clipboardCut && App.clipboardJson() === json)
                App.clearClipboard()
            Operations.moveTo(json, folder)
        }
        var timeout = App.setting("remorse_seconds") * 1000
        remorsePopup.execute(items.length === 1
                             //% "Moving “%1”"
                             ? qsTrId("lautta-dir-moving").arg(App.nameOf(items[0]))
                             //% "Moving %n items"
                             : qsTrId("lautta-dir-moving-many", items.length), action, timeout)
    }

    function folderMade(folderUri) {
        landed([folderUri], false)
        var items = toMove
        toMove = []
        if (items.length > 0)
            Operations.moveTo(JSON.stringify(items), folderUri)
    }

    function searchHere() {
        pageStack.push(Qt.resolvedUrl("../../pages/SearchPage.qml"), { "rootUri": uri })
    }

    function viewOptions() {
        pageStack.push(Qt.resolvedUrl("../../pages/ViewOptionsPage.qml"), { "model": dir })
    }

    function activity() {
        pageStack.push(Qt.resolvedUrl("../../pages/TransfersPage.qml"))
    }

    function moreActions(info) {
        moreActionsPage = pageStack.push(Qt.resolvedUrl("MoreActionsPage.qml"), { "owner": view, "info": info })
    }

    function moreSelection() {
        var uris = dir.selectedUris()
        if (uris.length === 0)
            return
        moreActionsPage = pageStack.push(Qt.resolvedUrl("MoreActionsPage.qml"), {
            "owner": view, "info": selectionInfo(uris), "selection": true, "count": uris.length
        })
    }

    function openDialog(path, properties) {
        var url = Qt.resolvedUrl(path)
        if (moreActionsPage && pageStack.currentPage === moreActionsPage)
            return pageStack.replace(url, properties)
        return pageStack.push(url, properties)
    }

    function remove(uris, listItem) {
        var action = function () {
            Operations.remove(JSON.stringify(uris))
            refreshTimer.restart()
            if (view.selecting)
                view.endSelection()
        }
        if (Operations.deletesPermanently(JSON.stringify(uris))) {
            var dialog = pageStack.push(Qt.resolvedUrl("../../dialogs/DeleteDialog.qml"), { "uris": uris })
            dialog.accepted.connect(action)
            return
        }
        var timeout = App.setting("remorse_seconds") * 1000
        if (listItem) {
            //% "Moving to trash"
            listItem.remorseAction(qsTrId("lautta-dir-trashing"), action, timeout)
        } else {
            settleRemorse()
            //% "Moving %n items to trash"
            remorsePopup.execute(qsTrId("lautta-dir-trashing-many", uris.length), action, timeout)
        }
    }

    function share(uris, mimeType) {
        if (uris.length === 1 && !App.isLocal(uris[0])) {
            var getting = openDialog("../../dialogs/OpenRemoteDialog.qml", { "uri": uris[0], "share": true })
            getting.fetched.connect(function (fileUrl) {
                shareAction.resources = [fileUrl]
                shareAction.mimeType = "*/*"
                shareAction.trigger()
            })
            return
        }
        var urls = []
        for (var i = 0; i < uris.length; ++i) {
            var url = App.localUrl(uris[i])
            if (url.length > 0)
                urls.push(url)
        }
        shareAction.resources = urls
        shareAction.mimeType = uris.length === 1 ? mimeType : "*/*"
        shareAction.trigger()
    }

    function runAction(id, uris, info, listItem) {
        switch (id) {
        case "share": share(uris, info.mimeType); break
        case "copy": App.copy(JSON.stringify(uris)); break
        case "cut": App.cut(JSON.stringify(uris)); break
        case "rename": rename(info); break
        case "delete": remove(uris, listItem); break
        case "more": moreActions(info); break
        case "moveToNewFolder": newFolder(uris); break
        case "compress": openDialog("../../dialogs/CompressDialog.qml", { "uris": uris }); break
        case "extract": Operations.extract(uris[0], ""); break
        case "checksums": openDialog("../../pages/ChecksumsPage.qml", { "uri": uris[0] }); break
        case "verify": openDialog("../../pages/VerifyFilesPage.qml", { "uri": uris[0] }); break
        case "favourite":
            openDialog("../../pages/FavouritePage.qml", { "uri": uris[0], "isDir": info.isDir !== false })
            break
        }
    }

    function selectionInfo(uris) {
        return { "uri": uris[0], "mimeType": "", "isDir": dir.kindOf(uris[0]) === "folder" }
    }

    function runSelectionAction(id) {
        var uris = dir.selectedUris()
        if (uris.length === 0)
            return
        runAction(id, uris, selectionInfo(uris), null)
        if (Selection.endsSelection(id))
            endSelection()
    }

    function hiddenTitle() {
        //% "Files here are hidden"
        return qsTrId("lautta-dir-hidden")
    }

    function hiddenHint() {
        //% "Files you add show in Activity, not here"
        return qsTrId("lautta-dir-hidden-hint")
    }

    function placeholderKind() {
        if (dir.count > 0 || dir.loading || atRoot)
            return ""
        if (dir.errorKind === "Sandbox" || dir.errorKind === "PermissionDenied")
            return "noaccess"
        if (dir.errorKind.length > 0)
            return "error"
        return dir.readable ? "empty" : "hidden"
    }

    function placeholderTitle(kind, errorKind) {
        switch (kind) {
        case "noaccess":
            //% "Not accessible"
            return qsTrId("lautta-dir-noaccess")
        case "empty":
            //% "No files"
            return qsTrId("lautta-dir-empty")
        case "hidden":
            return hiddenTitle()
        }
        switch (errorKind) {
        case "AuthFailed":
        case "ServerIdentityChanged":
            return ServerWarning.title(errorKind)
        case "NotFound":
            //% "Folder not found"
            return qsTrId("lautta-dir-err-not-found")
        case "NetworkUnreachable":
        case "TimedOut":
        case "ConnectionLost":
        case "BridgeUnavailable":
        case "LocationGone":
            //% "Can't reach this place"
            return qsTrId("lautta-dir-err-unreachable")
        }
        //% "Can't open this folder"
        return qsTrId("lautta-dir-err-generic")
    }

    function placeholderHint(kind, errorKind) {
        switch (kind) {
        case "noaccess":
            return errorKind === "Sandbox"
                   //% "This link points outside the folders Lautta may read."
                   ? qsTrId("lautta-dir-noaccess-hint")
                   //% "You don't have permission to open this folder."
                   : qsTrId("lautta-dir-noaccess-permission")
        case "hidden":
            return hiddenHint()
        case "empty":
            if (picking && dir.writable)
                //% "Save here, or pull down to create a folder"
                return qsTrId("lautta-dir-empty-hint-picking")
            if (!dir.writable)
                //% "This folder is empty"
                return qsTrId("lautta-dir-empty-hint-readonly")
            return canPaste
                   //% "Pull down to add or paste"
                   ? qsTrId("lautta-dir-empty-hint-paste")
                   //% "Pull down to add"
                   : qsTrId("lautta-dir-empty-hint")
        }
        var text = ServerWarning.level(errorKind)
                   ? ServerWarning.hint(errorKind, dir.locationName)
                   : ErrorText.message(errorKind, { "location": dir.locationName, "item": dir.title, "open": true })
        return detailsShown ? text + "\n\n" + dir.errorMessage : text
    }

    function placeholderButtons(kind, errorKind) {
        if (kind !== "error")
            return []
        var buttons = []
        if (ServerWarning.level(errorKind))
            buttons.push({ "text": ServerWarning.fixText(errorKind), "action": "account", "primary": true })
        else
            //% "Retry"
            buttons.push({ "text": qsTrId("lautta-dir-retry"), "action": "retry" })
        //% "Details"
        buttons.push({ "text": qsTrId("lautta-dir-details"), "action": "details" })
        return buttons
    }

    function placeholderAction(action) {
        switch (action) {
        case "retry": dir.refresh(); break
        case "details": detailsShown = !detailsShown; break
        case "account":
            Bridge.editAccount("lautta://" + uri.split("/")[2] + "/")
            break
        }
    }

    function notifyFailure(kind, message, item) {
        notice.show(ErrorText.message(kind, { "location": dir.locationName, "item": item }))
    }

    onGridModeChanged: fitGridRows()
    Component.onCompleted: fitGridRows()

    DirectoryModel {
        id: dir

        uri: view.uri
        onRenamed: view.renameDone(oldUri, newUri)
        onRenameFailed: {
            if (uri === view.undoingUri)
                view.undoingUri = ""
            view.notifyFailure(kind, message, view.pendingName)
        }
        onStateChanged: {
            view.revealRenamed()
            view.revealMade()
        }
        onCountChanged: view.fitGridRows()
        onCreated: view.folderMade(uri)
        onCreateFailed: {
            view.toMove = []
            view.notifyFailure(kind, message, view.pendingName)
        }
        onSelectedCountChanged: view.setSelection(Selection.afterCount({ "selecting": view.selectionMode,
                                                                          "waiting": view.selectionWaiting },
                                                                        selectedCount))
    }

    PathModel {
        id: path

        uri: view.uri
    }

    ShareAction {
        id: shareAction
    }

    Notice {
        id: notice
    }

    RemorsePopup {
        id: remorsePopup
    }

    Timer {
        id: renamedTimer

        onTriggered: view.forgetRename(view.renamed)
    }

    Timer {
        id: refreshTimer

        interval: 1200
        onTriggered: dir.refresh()
    }

    Connections {
        target: Transfers
        onLanded: view.landed(JSON.parse(urisJson), true)
        onFinished: if (view.visible && Qt.application.state === Qt.ApplicationActive) dir.revisit()
    }

    Connections {
        target: Operations
        onJobFinished: if (ok && resultUri) view.landed([resultUri], true)
    }

    ViewerPrefetch { id: prefetch }

    Component {
        id: scanMaker

        ScanModel { }
    }

    Component {
        id: listDelegate

        ListItem {
            id: item

            readonly property var info: ({
                "uri": model.uri,
                "isDir": model.isDir,
                "size": model.size,
                "modified": model.modified,
                "category": model.category,
                "mimeType": model.mimeType
            })
            readonly property bool transferring: model.transferState.length > 0
            property Component contextMenu: Component {
                FileContextMenu {
                    owner: view
                    listItem: item
                    info: item.info
                }
            }

            contentHeight: Theme.itemSizeMedium
            menu: view.selecting || view.picking || model.inaccessible ? null : contextMenu
            enabled: !model.inaccessible && view.offered(model.isDir)
            opacity: enabled ? 1 : Theme.opacityLow
            onPressed: view.itemPressed(model.uri, model.isDir, model.mimeType)
            onClicked: {
                if (view.selecting)
                    dir.toggle(model.uri)
                else
                    view.openItem(model.uri, model.isDir, model.mimeType, icon.thumbnailReady ? icon.size : 0)
            }
            onPressAndHold: {
                prefetch.stop()
                if (!view.selecting)
                    return
                DelegateModel.inPersistedItems = true
                dragSelect.begin(item, index, mouse.x, mouse.y)
            }
            onPositionChanged: if (dragSelect.holder === item) dragSelect.move(mouse.x, mouse.y)
            onReleased: item.letGo()
            onCanceled: {
                prefetch.stop()
                item.letGo()
            }

            function letGo() {
                if (dragSelect.holder !== item)
                    return
                dragSelect.end()
                DelegateModel.inPersistedItems = false
            }

            Rectangle {
                anchors.fill: parent
                visible: model.selected
                color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 2)
            }

            RenameRemorse {
                owner: view
                row: item
                uri: model.uri
            }

            NewItemFlash {
                anchors.fill: parent
                owner: view
                uri: model.uri
            }

            FileIcon {
                id: icon

                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                size: Theme.iconSizeMedium
                category: model.category
                isDir: model.isDir
                isSymlink: model.isSymlink
                thumbnailSource: model.thumbnailSource
                mimeType: model.mimeType
                selected: model.selected
                highlighted: item.highlighted || model.selected
            }

            Column {
                anchors {
                    left: icon.right
                    leftMargin: Theme.paddingLarge
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Row {
                    width: parent.width
                    spacing: Theme.paddingSmall

                    Label {
                        width: parent.width - (lossyBadge.visible ? lossyBadge.width + parent.spacing : 0)
                        text: model.name
                        truncationMode: TruncationMode.Fade
                        font.pixelSize: Theme.fontSizeMedium
                        color: item.highlighted || model.selected ? Theme.highlightColor : Theme.primaryColor
                    }
                    Rectangle {
                        id: lossyBadge

                        visible: model.nameIsLossy
                        anchors.verticalCenter: parent.verticalCenter
                        width: Theme.fontSizeExtraSmall * 1.4
                        height: width
                        radius: 3
                        color: "transparent"
                        border {
                            width: 1
                            color: Theme.secondaryColor
                        }

                        Label {
                            anchors.centerIn: parent
                            text: "?"
                            font.pixelSize: Theme.fontSizeTiny
                            color: Theme.secondaryColor
                        }
                    }
                }

                Label {
                    width: parent.width
                    text: item.transferring ? DirFormat.transferText(model.transferState, dir.locationName)
                                            : model.inaccessible
                                              //% "Not accessible"
                                              ? qsTrId("lautta-dir-row-not-accessible")
                                              : DirFormat.subtitle(model.isDir, model.isSymlink, model.size, model.modified)
                    visible: text.length > 0
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                }

                Rectangle {
                    visible: item.transferring
                    width: parent.width
                    height: Theme.paddingSmall / 2
                    color: Theme.rgba(Theme.highlightColor, 0.3)

                    Rectangle {
                        width: parent.width * DirFormat.transferPercent(model.transferState)
                        height: parent.height
                        color: Theme.highlightColor
                    }
                }
            }
        }
    }

    Component {
        id: gridDelegate

        BackgroundItem {
            id: cell

            readonly property string itemName: model.name
            readonly property int items: model.itemCount
            readonly property bool counting: model.isDir && dir.folderCounts
            readonly property string countKey: counting + "/" + model.modified + "/" + dir.showHidden
            readonly property string previewKey: model.textThumb ? model.uri + "/" + model.modified + "/" + model.size : ""
            property string previewAsked
            readonly property var captions: DirFormat.gridCaptions(model.isDir, model.size, model.modified,
                                                                   dir.folderCounts, items, model.itemCountCapped)
            readonly property real gap: gridMetrics.gap

            x: (index % grid.columns) * width
            width: grid.cellWidth
            height: parent ? parent.height : implicitHeight
            enabled: view.offered(model.isDir)
            opacity: enabled ? 1 : Theme.opacityLow
            implicitHeight: gridMetrics.cellHeight(width, caption.implicitHeight)
            onCountKeyChanged: cell.countItems()
            onPreviewKeyChanged: cell.askPreview()
            Component.onCompleted: {
                cell.countItems()
                cell.askPreview()
            }
            Component.onDestruction: {
                if (previewAsked)
                    dir.cancelPreview(previewAsked)
            }
            onPressed: view.itemPressed(model.uri, model.isDir, model.mimeType)
            onClicked: {
                if (view.selecting)
                    dir.toggle(model.uri)
                else
                    view.openItem(model.uri, model.isDir, model.mimeType, tileIcon.thumbnailReady ? Math.round(tileIcon.width) : 0)
            }
            onPressAndHold: {
                prefetch.stop()
                if (dir.readable && !view.picking)
                    dragSelect.begin(cell, index, mouse.x, mouse.y)
            }
            onPositionChanged: if (dragSelect.holder === cell) dragSelect.move(mouse.x, mouse.y)
            onReleased: cell.letGo()
            onCanceled: {
                prefetch.stop()
                cell.letGo()
            }

            function countItems() {
                if (counting)
                    dir.requestCount(model.uri)
            }

            function askPreview() {
                if (previewAsked && previewAsked !== model.uri)
                    dir.cancelPreview(previewAsked)
                previewAsked = model.textThumb ? model.uri : ""
                if (previewAsked)
                    dir.requestPreview(previewAsked)
            }

            function letGo() {
                if (dragSelect.holder === cell)
                    dragSelect.end()
            }

            RenameRemorse {
                owner: view
                row: cell
                uri: model.uri
            }

            Item {
                id: tile

                x: cell.gap
                y: cell.gap
                width: grid.cellWidth - 2 * cell.gap
                height: width

                Rectangle {
                    anchors.fill: parent
                    visible: !tileIcon.thumbnailReady
                    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 3)
                }

                FileIcon {
                    id: tileIcon

                    anchors.fill: parent
                    size: tile.width
                    glyphSize: gridMetrics.glyphSize(tile.width)
                    category: model.category
                    isDir: model.isDir
                    isSymlink: model.isSymlink
                    thumbnailSource: model.thumbnailSource
                    mimeType: model.mimeType
                    selected: model.selected
                    previewText: model.textPreview
                    previewMono: model.textPreviewMono
                    typeIcon: model.typeIcon
                    highlighted: cell.highlighted || model.selected
                }

                NewItemFlash {
                    id: tileFlash

                    anchors.fill: parent
                    owner: view
                    uri: model.uri
                }
            }

            GridCaption {
                id: caption

                anchors {
                    top: tile.bottom
                    topMargin: Theme.paddingSmall / 2
                    left: tile.left
                    right: tile.right
                }
                metrics: gridMetrics
                name: cell.itemName
                captions: cell.captions
                minLines: cell.counting && !model.inaccessible ? 1 : 0
                emphasised: cell.highlighted || model.selected
                pressed: cell.highlighted
                glow: tileFlash.opacity
            }
        }
    }

    Component {
        id: gridRowDelegate

        Item {
            id: gridRow

            readonly property int first: index * grid.columns
            readonly property int end: (index + 1) * grid.columns
            readonly property bool holding: !!dragSelect.holder && dragSelect.holder.parent === gridRow

            function regroup() {
                var all = rowCells.items
                var span = GridRows.span(index, grid.columns, all.count)
                var kept = []
                for (var i = inRow.count - 1; i >= 0; --i) {
                    var at = inRow.get(i).itemsIndex
                    if (at < span.from || at >= span.to)
                        inRow.remove(i, 1)
                    else
                        kept.push(at)
                }
                for (var j = span.from; j < span.to; ++j) {
                    if (kept.indexOf(j) < 0)
                        all.addGroups(j, 1, "inRow")
                }
            }

            width: grid.width
            height: GridRows.tallest(children)
            onEndChanged: regroup()
            onHoldingChanged: DelegateModel.inPersistedItems = holding
            Component.onCompleted: regroup()

            DelegateModel {
                id: rowCells

                model: dir
                delegate: gridDelegate
                groups: DelegateModelGroup {
                    id: inRow

                    name: "inRow"
                }
                filterOnGroup: "inRow"
            }

            Connections {
                target: rowCells.items
                onChanged: gridRow.regroup()
            }

            Repeater {
                model: rowCells
            }
        }
    }

    GridMetrics {
        id: gridMetrics
    }

    ListModel {
        id: gridRows
    }

    SilicaFlickable {
        id: pulleyHost

        objectName: "folderPulleyHost"
        anchors.fill: parent
        contentWidth: width
        contentHeight: height

        DirectoryMenu {
            owner: view
        }

        Item {
            id: top

            objectName: "folderHeader"
            z: 1
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }
            height: errorNotice.y + errorNotice.height

            HeaderBand {
                objectName: "folderHeaderBand"
                anchors {
                    top: parent.top
                    bottom: accessNotice.bottom
                }
            }

            Item {
                id: leadSlot

                width: parent.width
                height: childrenRect.height
            }

            Item {
                id: headerBar

                objectName: "folderHeaderBar"
                anchors.top: leadSlot.bottom
                width: parent.width
                height: view.picking ? band.space : pageHeader.height + crumbs.menuHeight

                Loader {
                    id: band

                    readonly property real space: item ? item.height + item.menuHeight : 0

                    width: parent.width
                    active: view.picking
                    sourceComponent: PathBand {
                        objectName: "folderBand"
                        title: view.atRoot ? view.rootTitle : dir.title
                        pathModel: path
                        menuHost: headerBar
                        menuMost: Math.max(0, Math.floor((view.height - leadSlot.height) / Theme.itemSizeSmall) - 1)
                        homeText: view.rootTitle
                        onPicked: view.ancestorPicked(uri)
                        onHome: view.homePicked()
                    }
                }

                PageHeader {
                    id: pageHeader

                    objectName: "folderPageHeader"
                    visible: !view.picking
                    leftMargin: view.backTapWidth
                    description: !view.selecting && Operations.isArchive(view.uri)
                                 //% "Compressed file · read only"
                                 ? qsTrId("lautta-dir-compressed-readonly")
                                 : ""
                    extraContent.children: [
                        PathCrumbs {
                            id: crumbs

                            objectName: "folderCrumbs"
                            anchors.fill: parent
                            title: view.selecting
                                   //% "%n selected"
                                   ? qsTrId("lautta-dir-selected", dir.selectedCount)
                                   : dir.title
                            pathShown: !view.selecting
                            pathModel: path
                            titleTop: pageHeader.topMargin
                            menuHost: headerBar
                            menuMost: Math.max(0, Math.floor((view.height - pageHeader.height) / Theme.itemSizeSmall))
                            homeInset: view.width / 4 - view.backTapWidth
                            onPicked: view.ancestorPicked(uri)
                            onHome: view.homePicked()
                        }
                    ]
                }
            }

            SlimProgress {
                id: progress

                anchors.top: headerBar.bottom
                running: dir.loading
            }

            Item {
                id: accessNotice

                objectName: "folderAccessNotice"
                anchors.top: progress.bottom
                width: parent.width
                visible: dir.access.length > 0
                height: visible ? accessRow.height + Theme.paddingMedium : 0

                Row {
                    id: accessRow

                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: Theme.paddingSmall

                    HighlightImage {
                        anchors.verticalCenter: parent.verticalCenter
                        source: "image://theme/icon-s-secure"
                        color: Theme.secondaryHighlightColor
                    }
                    Label {
                        objectName: "folderAccessLevel"
                        anchors.verticalCenter: parent.verticalCenter
                        text: Access.name(dir.access)
                        font.pixelSize: Theme.fontSizeSmall
                        color: Theme.secondaryHighlightColor
                    }
                }
            }

            InfoBanner {
                id: offlineNotice

                anchors.top: accessNotice.bottom
                visible: dir.offline
                height: visible ? implicitHeight : 0
                //% "Offline. Showing the copy from %1. Changes aren't possible."
                text: qsTrId("lautta-dir-offline").arg(dir.cachedAt >= 0
                      ? DirFormat.modified(dir.cachedAt) : "")
                //% "Retry"
                actionText: qsTrId("lautta-dir-offline-retry")
                onAction: dir.refresh()
            }

            InfoBanner {
                id: largeNotice

                anchors.top: offlineNotice.bottom
                visible: dir.large
                height: visible ? implicitHeight : 0
                //% "%1 loaded · large folder: name order, thumbnails off"
                text: qsTrId("lautta-dir-large").arg(dir.count)
                //% "Stop"
                actionText: dir.loading ? qsTrId("lautta-dir-large-stop") : ""
                onAction: dir.stopLoading()
            }

            InfoBanner {
                id: gridNotice

                anchors.top: largeNotice.bottom
                visible: dir.suggestGrid
                height: visible ? implicitHeight : 0
                //% "Shown as grid because most items are photos"
                text: qsTrId("lautta-dir-grid-suggested")
                //% "Use list"
                actionText: qsTrId("lautta-dir-use-list")
                onAction: dir.setViewPrefs(JSON.stringify({ "viewMode": "list" }))
            }

            InfoBanner {
                id: errorNotice

                anchors.top: gridNotice.bottom
                visible: dir.errorKind.length > 0 && dir.count > 0 && !dir.offline
                height: visible ? implicitHeight : 0
                text: ErrorText.message(dir.errorKind, { "location": dir.locationName, "item": dir.title, "open": true })
                actionText: qsTrId("lautta-dir-retry")
                onAction: dir.refresh()
            }
        }

        SilicaListView {
            id: list

            function itemIndexAt(x, y) {
                return indexAt(x, y)
            }

            anchors {
                top: top.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            clip: true
            visible: !view.gridMode && !view.atRoot
            model: view.gridMode ? null : dir
            delegate: listDelegate
            footer: Item {
                width: list.width
                height: (hiddenNote.visible ? hiddenNote.y + hiddenNote.height : 0) + panelSpace.height

                FolderPlaceholder {
                    id: hiddenNote

                    visible: !dir.readable && dir.count > 0
                    x: Theme.horizontalPageMargin
                    y: Theme.itemSizeLarge
                    title: view.hiddenTitle()
                    hint: view.hiddenHint()
                }
            }

            VerticalScrollDecorator { }
        }

        SilicaListView {
            id: grid

            readonly property real tileWidth: Math.min(Screen.width, Screen.height) / gridMetrics.portraitColumns
            readonly property int columns: GridRows.columns(width, tileWidth, gridMetrics.portraitColumns)
            readonly property real cellWidth: width / columns

            function itemIndexAt(x, y) {
                return GridRows.indexAt(indexAt(x, y), x, cellWidth, columns, dir.count)
            }

            anchors {
                top: top.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            clip: true
            visible: view.gridMode && !view.atRoot
            model: gridRows
            delegate: gridRowDelegate
            onColumnsChanged: view.fitGridRows()
            footer: Item {
                width: grid.width
                height: (hiddenNote.visible ? hiddenNote.y + hiddenNote.height : 0) + panelSpace.height

                FolderPlaceholder {
                    id: hiddenNote

                    visible: !dir.readable && dir.count > 0
                    x: Theme.horizontalPageMargin
                    y: Theme.itemSizeLarge
                    title: view.hiddenTitle()
                    hint: view.hiddenHint()
                }
            }

            VerticalScrollDecorator { }
        }

        Item {
            id: placesSlot

            anchors {
                top: top.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            visible: view.atRoot
        }
    }

    Item {
        id: panelSpace

        width: 1
        height: (view.selecting || clipboardPanel.open ? Theme.itemSizeLarge : 0) + activityBar.space
    }

    Item {
        id: stateLayer

        objectName: "stateLayer"
        parent: view.flickable.contentItem
        x: view.flickable.originX
        y: view.flickable.originY
        width: view.flickable.width
        height: view.flickable.height

        BusyIndicator {
            anchors.centerIn: parent
            size: BusyIndicatorSize.Large
            running: dir.loading && dir.count === 0 && dir.errorKind.length === 0 && view.visible
        }

        FolderPlaceholder {
            id: placeholder

            readonly property string kind: view.placeholderKind()

            objectName: "folderPlaceholder"
            visible: kind.length > 0
            x: Theme.horizontalPageMargin
            y: Math.max(0, Math.min(Math.round(parent.height * 0.38), parent.height - height - Theme.paddingLarge))
            level: ServerWarning.level(dir.errorKind)
            title: view.placeholderTitle(kind, dir.errorKind)
            hint: view.placeholderHint(kind, dir.errorKind)
            buttons: view.placeholderButtons(kind, dir.errorKind)
            onTriggered: view.placeholderAction(action)
        }
    }

    DragSelect {
        id: dragSelect

        view: view.flickable
        model: dir
        bottomInset: selectionPanel.visibleSize
    }

    SelectionPanel {
        id: selectionPanel

        owner: view
    }

    ClipboardPanel {
        id: clipboardPanel

        owner: view
    }

    ActivityBar {
        id: activityBar

        allowed: !view.selecting && !view.picking
        anchors.bottomMargin: clipboardPanel.visibleSize
    }
}
