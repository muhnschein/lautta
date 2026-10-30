// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import QtQml.Models 2.2
import Sailfish.Silica 1.0
import Sailfish.Share 1.0
import Lautta 1.0
import "../"
import "../transfers"
import "../Open.js" as Open
import "../ErrorText.js" as ErrorText
import "DirFormat.js" as DirFormat
import "GridRows.js" as GridRows
import "Selection.js" as Selection

Item {
    id: view

    property string uri
    property bool selectionMode
    property bool selectionWaiting
    property bool detailsShown
    property alias model: dir
    readonly property bool selecting: selectionMode
    readonly property bool gridMode: dir.viewMode === "grid"
    property string pendingName
    property string pendingShare
    readonly property Item flickable: gridMode ? grid : list
    property var renamed: null
    property bool renamingDir
    property string undoingUri
    property Item moreActionsPage
    property var toMove: []

    signal openFolder(string uri)

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

    function openItem(itemUri, isDir, mimeType) {
        if (isDir)
            openFolder(itemUri)
        else
            Open.open(pageStack, App, Operations, itemUri, false, mimeType, uri)
    }

    function newFolder(moving) {
        var dialog = openDialog("../../dialogs/NewItemDialog.qml", { "parentUri": uri })
        dialog.accepted.connect(function () {
            view.pendingName = dialog.itemName
            view.toMove = moving || []
            dir.createFolder(dialog.itemName)
        })
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

    // Silica keeps one remorse item per row for all its timers: `action` belongs to this one only.
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
        flickable.positionViewAtIndex(gridMode ? GridRows.rowOf(index, grid.columns) : index, ListView.Contain)
    }

    // Rows come and go at the end only: a new count as model would start grid anew at its top.
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

    // Popup holds one remorse: a new one first runs the pending one, as Silica does when page goes.
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

    // Trash after remorse timer; for good (volumes, servers, Trash off) once dialog accepted, no
    // timer.
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
            pendingShare = uris[0]
            App.prepareExternal(uris[0])
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
        // CompressDialog takes the list itself, not its JSON text.
        case "compress": openDialog("../../dialogs/CompressDialog.qml", { "uris": uris }); break
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

    function placeholderKind() {
        if (dir.count > 0 || dir.loading)
            return ""
        if (dir.errorKind === "Sandbox" || dir.errorKind === "PermissionDenied")
            return "noaccess"
        if (dir.errorKind.length > 0)
            return "error"
        return "empty"
    }

    function placeholderTitle(kind) {
        switch (kind) {
        case "noaccess":
            //% "Not accessible"
            return qsTrId("lautta-dir-noaccess")
        case "empty":
            //% "No files"
            return qsTrId("lautta-dir-empty")
        }
        switch (dir.errorKind) {
        case "AuthFailed":
            //% "Sign-in failed"
            return qsTrId("lautta-dir-err-auth")
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

    function placeholderHint(kind) {
        switch (kind) {
        case "noaccess":
            return dir.errorKind === "Sandbox"
                   //% "This link points outside the folders Lautta may read."
                   ? qsTrId("lautta-dir-noaccess-hint")
                   //% "You don't have permission to open this folder."
                   : qsTrId("lautta-dir-noaccess-permission")
        case "empty":
            return dir.writable
                   //% "Pull down to create a folder or paste"
                   ? qsTrId("lautta-dir-empty-hint")
                   //% "This folder is empty"
                   : qsTrId("lautta-dir-empty-hint-readonly")
        }
        var text = ErrorText.message(dir.errorKind, { "location": dir.locationName, "item": dir.title })
        return detailsShown ? text + "\n\n" + dir.errorMessage : text
    }

    function placeholderButtons(kind) {
        if (kind !== "error")
            return []
        var buttons = []
        if (dir.errorKind === "AuthFailed")
            //% "Update sign-in"
            buttons.push({ "text": qsTrId("lautta-dir-update-sign-in"), "action": "signIn" })
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
        case "signIn":
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
        onStateChanged: view.revealRenamed()
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

    // Server folder not watched: relisted when a transfer ends.
    Connections {
        target: Transfers
        onFinished: if (view.visible && Qt.application.state === Qt.ApplicationActive) dir.revisit()
    }

    Connections {
        target: App
        onExternalReady: {
            if (uri === view.pendingShare) {
                view.pendingShare = ""
                shareAction.resources = [fileUrl]
                shareAction.mimeType = "*/*"
                shareAction.trigger()
            }
        }
        onExternalFailed: {
            if (uri === view.pendingShare) {
                view.pendingShare = ""
                view.notifyFailure(kind, message, App.nameOf(uri))
            }
        }
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
            menu: view.selecting || model.inaccessible ? null : contextMenu
            enabled: !model.inaccessible
            opacity: model.inaccessible ? Theme.opacityLow : 1
            onClicked: {
                if (view.selecting)
                    dir.toggle(model.uri)
                else
                    view.openItem(model.uri, model.isDir, model.mimeType)
            }
            onPressAndHold: {
                if (!view.selecting)
                    return
                DelegateModel.inPersistedItems = true
                dragSelect.begin(item, index, mouse.x, mouse.y)
            }
            onPositionChanged: if (dragSelect.holder === item) dragSelect.move(mouse.x, mouse.y)
            onReleased: item.letGo()
            onCanceled: item.letGo()

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
            onClicked: {
                if (view.selecting)
                    dir.toggle(model.uri)
                else
                    view.openItem(model.uri, model.isDir, model.mimeType)
            }
            onPressAndHold: dragSelect.begin(cell, index, mouse.x, mouse.y)
            onPositionChanged: if (dragSelect.holder === cell) dragSelect.move(mouse.x, mouse.y)
            onReleased: cell.letGo()
            onCanceled: cell.letGo()

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
            }
        }
    }

    // Measures only rows the view makes, never every name of a large folder.
    Component {
        id: gridRowDelegate

        Item {
            id: gridRow

            readonly property int first: index * grid.columns
            readonly property int end: (index + 1) * grid.columns
            // Stays while a drag holds its cell, even out of view (DragSelect).
            readonly property bool holding: !!dragSelect.holder && dragSelect.holder.parent === gridRow

            // `first` may not have followed columns yet.
            function regroup() {
                var all = rowCells.items
                var from = index * grid.columns
                var to = Math.min(from + grid.columns, all.count)
                var kept = []
                for (var i = inRow.count - 1; i >= 0; --i) {
                    var at = inRow.get(i).itemsIndex
                    if (at < from || at >= to)
                        inRow.remove(i, 1)
                    else
                        kept.push(at)
                }
                for (var j = from; j < to; ++j) {
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

    // PullDownMenu draws at top of its flickable: on list or grid it would come out under header.
    // Host never scrolls, so header stays; list and grid stop at top and a pull goes on to host.
    SilicaFlickable {
        id: pulleyHost

        objectName: "folderPulleyHost"
        anchors.fill: parent
        // A flickable's content is 0x0 until told; anchors to parent go to it.
        contentWidth: width
        contentHeight: height

        DirectoryMenu {
            owner: view
        }

        // Not the list's header: list keeps contentY when header grows, so a late notice scrolled
        // list from top. Stacked by anchors, not Column: height follows at once, not next polish.
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

            Item {
                id: headerBar

                width: parent.width
                height: pageHeader.height

                PageHeader {
                    id: pageHeader

                    objectName: "folderPageHeader"
                    title: view.selecting
                           //% "%n selected"
                           ? qsTrId("lautta-dir-selected", dir.selectedCount)
                           : dir.title
                }
                MouseArea {
                    anchors.fill: parent
                    enabled: !view.selecting
                    onClicked: pathMenu.open = !pathMenu.open
                }
            }

            SlimProgress {
                id: progress

                anchors.top: headerBar.bottom
                running: dir.loading
            }

            PathMenu {
                id: pathMenu

                objectName: "folderPathMenu"
                anchors.top: progress.bottom
                maxHeight: Math.max(0, view.height - y - Theme.itemSizeLarge)
                pathModel: path
                currentUri: view.uri
                onNavigate: view.openFolder(uri)
                onFavouriteRequested: view.runAction("favourite", [view.uri], {}, null)
            }

            InfoBanner {
                id: offlineNotice

                anchors.top: pathMenu.bottom
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
                text: ErrorText.message(dir.errorKind, { "location": dir.locationName, "item": dir.title })
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
            visible: !view.gridMode
            model: view.gridMode ? null : dir
            delegate: listDelegate
            footer: Item {
                width: list.width
                height: panelSpace.height
            }

            VerticalScrollDecorator { }
        }

        // GridView gives every cell one height.
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
            visible: view.gridMode
            model: gridRows
            delegate: gridRowDelegate
            onColumnsChanged: view.fitGridRows()
            footer: Item {
                width: grid.width
                height: panelSpace.height
            }

            VerticalScrollDecorator { }
        }
    }

    Item {
        id: panelSpace

        width: 1
        height: (view.selecting || App.clipboardCount > 0 ? Theme.itemSizeLarge : 0) + activityBar.space
    }

    // States sit in content of list or grid, as Silica's ViewPlaceholder: pulley moves them with
    // content. As siblings they stayed put.
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
            y: Math.round(parent.height * 0.38)
            title: view.placeholderTitle(kind)
            hint: view.placeholderHint(kind)
            buttons: view.placeholderButtons(kind)
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

        allowed: !view.selecting
        anchors.bottomMargin: clipboardPanel.visibleSize
    }
}
