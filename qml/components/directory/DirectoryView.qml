// SPDX-License-Identifier: LGPL-2.1-or-later
// The folder view of DirectoryPage (SPEC §9, §15.3): header with the path menu, list or grid, pulley,
// context menu and its More actions page, selection mode with drag
// selection and a docked panel, paste bar, remorse (also the undo of a
// rename, OPS-9) and the placeholders for empty, not accessible, offline,
// large and error.
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
import "Selection.js" as Selection

Item {
    id: view

    property string uri
    property bool selectionMode
    // True while selection mode waits for the first item (pulley Select).
    property bool selectionWaiting
    property bool detailsShown
    property alias model: dir
    readonly property bool selecting: selectionMode
    readonly property bool gridMode: dir.viewMode === "grid"
    // Grid items per row in portrait (View options): 4, or 3 with the name
    // and caption lines under every tile (SPEC §15.3).
    readonly property int gridColumns: {
        var dependency = App.settingsJson
        return App.setting("grid_columns") === 3 ? 3 : 4
    }
    readonly property bool gridCaptions: gridColumns === 3
    property string pendingName
    property string pendingShare
    // The list or the grid, whichever is shown.
    readonly property Item flickable: gridMode ? grid : list
    // The item renamed last while its remorse runs (OPS-9): { uri, oldUri,
    // isDir, ends, revealed }. Its row shows the remorse (RenameRemorse.qml).
    property var renamed: null
    property bool renamingDir
    // The item renamed back from its remorse: that rename has none.
    property string undoingUri
    // The More actions page while it is open.
    property Item moreActionsPage

    // Asks the host page to open a folder.
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

    function newFolder() {
        var dialog = pageStack.push(Qt.resolvedUrl("../../dialogs/NewItemDialog.qml"), { "parentUri": uri })
        dialog.accepted.connect(function () {
            view.pendingName = dialog.itemName
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

    // A rename is undone from a remorse on its row, not a banner (OPS-9).
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

    // Shows the remorse of the rename on `row` (a list row or grid cell) for
    // the time left. Returns { rename, orphaned }; the row sets `orphaned`
    // when it goes: its remorse ends with it then, and that is not running
    // out.
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

    // Silica keeps one remorse item per row for all its timers: `action`
    // belongs to this one only.
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

    // Tapping the remorse renames the item back.
    function undoRename(r) {
        forgetRename(r)
        undoingUri = r.uri
        pendingName = App.nameOf(r.oldUri)
        dir.rename(r.uri, pendingName)
    }

    // The renamed item may have moved out of sight: bring its row, and so
    // its remorse, into view once the listing has it.
    function revealRenamed() {
        if (!renamed || renamed.revealed || dir.loading)
            return
        var index = dir.indexOf(renamed.uri)
        if (index < 0)
            return
        renamed.revealed = true
        flickable.positionViewAtIndex(index, ListView.Contain)
    }

    function paste() {
        var taken = JSON.parse(App.takeClipboard())
        if (!taken)
            return
        if (taken.cut)
            Operations.moveTo(JSON.stringify(taken.items), uri)
        else
            Operations.copyTo(JSON.stringify(taken.items), uri)
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

    // The More actions page of the selected items (SPEC §15.3, OPS-12),
    // opened by the panel's More actions button.
    function moreSelection() {
        var uris = dir.selectedUris()
        if (uris.length === 0)
            return
        moreActionsPage = pageStack.push(Qt.resolvedUrl("MoreActionsPage.qml"), {
            "owner": view, "info": selectionInfo(uris), "selection": true, "count": uris.length
        })
    }

    // A dialog opened from More actions takes that page's place, so that
    // leaving the dialog returns to the folder.
    function openDialog(path, properties) {
        var url = Qt.resolvedUrl(path)
        if (moreActionsPage && pageStack.currentPage === moreActionsPage)
            return pageStack.replace(url, properties)
        return pageStack.push(url, properties)
    }

    // OPS-8: to the Trash after the remorse timer; for good (volumes,
    // servers, the Trash off) once the dialog is accepted, with no timer.
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

    // One action on one or more items; `info` describes the first item. A
    // favourite is the item itself (ORG-1): a folder unless `info` says it
    // is a file (the path menu's favourite is the folder shown).
    function runAction(id, uris, info, listItem) {
        switch (id) {
        case "share": share(uris, info.mimeType); break
        case "copy": App.copy(JSON.stringify(uris)); break
        case "cut": App.cut(JSON.stringify(uris)); break
        case "rename": rename(info); break
        case "delete": remove(uris, listItem); break
        case "more": moreActions(info); break
        // CompressDialog takes the list itself, not its JSON text.
        case "compress": openDialog("../../dialogs/CompressDialog.qml", { "uris": uris }); break
        case "favourite":
            openDialog("../../pages/FavouritePage.qml", { "uri": uris[0], "isDir": info.isDir !== false })
            break
        }
    }

    // The first of the selected `uris` as a row describes it; Rename and Add
    // to favourites need to know whether their one item is a folder.
    function selectionInfo(uris) {
        return { "uri": uris[0], "mimeType": "", "isDir": dir.kindOf(uris[0]) === "folder" }
    }

    // Actions of the selection panel and of its More actions page (board
    // DirectorySelect).
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
            // The sign-in is netvfs' to update, as from the Browse menu (NVB-5).
            Bridge.editAccount("lautta://" + uri.split("/")[2] + "/")
            break
        }
    }

    function notifyFailure(kind, message, item) {
        notice.show(ErrorText.message(kind, { "location": dir.locationName, "item": item }))
    }

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
        onCreateFailed: view.notifyFailure(kind, message, view.pendingName)
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

    // The remorse of the rename ran out, shown or not (its row may be out
    // of view): the new name stays.
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

    // The folder's header, its progress line, path menu and notices stay at
    // the top of the page while the list or grid scrolls under them (SPEC
    // §15.3). They are not the header of the list: a list keeps its contentY
    // when its header grows, so a notice or a line that came after the rows
    // scrolled the list away from its top. Stacked by anchors, not a Column:
    // the height follows a notice or a line at once, not at the next polish.
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
                // No breadcrumb (as in iOS Files): the path menu shows the way up.
                description: view.selecting ? dir.title : ""
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
            // The rest of the page less a strip of the list; longer, it scrolls.
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
            // Outside selection the long-press menu opens; while selecting,
            // holding flips the item and a drag goes on as in the grid.
            onPressAndHold: {
                if (!view.selecting)
                    return
                DelegateModel.inPersistedItems = true
                dragSelect.begin(item, index, mouse.x, mouse.y)
            }
            onPositionChanged: if (dragSelect.holder === item) dragSelect.move(mouse.x, mouse.y)
            onReleased: item.letGo()
            onCanceled: item.letGo()

            // Last: the view drops a row out of view once it is not held.
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

            // Three per row: a folder here (device, drives) shows its item
            // count, asked for when the cell appears and again when the
            // folder or the hidden files setting changed.
            readonly property string itemName: model.name
            readonly property int items: model.itemCount
            readonly property bool counting: view.gridCaptions && model.isDir && dir.folderCounts
            readonly property string countKey: counting + "/" + model.modified + "/" + dir.showHidden
            readonly property var captions: view.gridCaptions
                                            ? DirFormat.gridCaptions(model.isDir, model.size, model.modified,
                                                                     dir.folderCounts, items, model.itemCountCapped)
                                            : []
            readonly property real gap: gridMetrics.gap(view.gridCaptions)

            width: grid.cellWidth
            height: grid.cellHeight
            onCountKeyChanged: cell.countItems()
            Component.onCompleted: cell.countItems()
            onClicked: {
                if (view.selecting)
                    dir.toggle(model.uri)
                else
                    view.openItem(model.uri, model.isDir, model.mimeType)
            }
            // As in Photos apps, holding an item selects it (starting selection
            // mode) and the finger then selects a range (DragSelect, §15.3).
            // The view keeps a held item while its row scrolls out of view.
            onPressAndHold: {
                DelegateModel.inPersistedItems = true
                dragSelect.begin(cell, index, mouse.x, mouse.y)
            }
            onPositionChanged: if (dragSelect.holder === cell) dragSelect.move(mouse.x, mouse.y)
            onReleased: cell.letGo()
            onCanceled: cell.letGo()

            function countItems() {
                if (counting)
                    dir.requestCount(model.uri)
            }

            // Last: the view drops an item out of view once it is not held.
            function letGo() {
                if (dragSelect.holder !== cell)
                    return
                dragSelect.end()
                DelegateModel.inPersistedItems = false
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
                    glyphSize: gridMetrics.glyphSize(tile.width, view.gridCaptions)
                    category: model.category
                    isDir: model.isDir
                    isSymlink: model.isSymlink
                    thumbnailSource: model.thumbnailSource
                    mimeType: model.mimeType
                    selected: model.selected
                }

                // Compact grid: the name inside the tile, cut at the end only.
                Label {
                    visible: !view.gridCaptions && (model.thumbnailSource.length === 0 || tileIcon.thumbnailFailed)
                    anchors {
                        left: parent.left
                        right: parent.right
                        bottom: parent.bottom
                        margins: Theme.paddingSmall
                    }
                    horizontalAlignment: Text.AlignHCenter
                    text: model.name
                    truncationMode: TruncationMode.Elide
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: cell.highlighted || model.selected ? Theme.highlightColor : Theme.primaryColor
                }
            }

            GridCaption {
                visible: view.gridCaptions
                anchors {
                    top: tile.bottom
                    topMargin: Theme.paddingSmall / 2
                    left: tile.left
                    right: tile.right
                }
                metrics: gridMetrics
                name: cell.itemName
                captions: cell.captions
                emphasised: cell.highlighted || model.selected
                pressed: cell.highlighted
            }
        }
    }

    GridMetrics {
        id: gridMetrics
    }

    SilicaListView {
        id: list

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

        DirectoryMenu {
            owner: view
        }

        VerticalScrollDecorator { }
    }

    SilicaGridView {
        id: grid

        // Landscape keeps the tile size of portrait.
        readonly property real tileWidth: Math.min(Screen.width, Screen.height) / view.gridColumns
        readonly property int columns: Math.max(view.gridColumns, Math.round(width / tileWidth))

        anchors {
            top: top.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        clip: true
        visible: view.gridMode
        model: view.gridMode ? dir : null
        cellWidth: width / columns
        cellHeight: gridMetrics.cellHeight(cellWidth, view.gridCaptions)
        delegate: gridDelegate
        footer: Item {
            width: grid.width
            height: panelSpace.height
        }

        DirectoryMenu {
            owner: view
        }

        VerticalScrollDecorator { }
    }

    // Room for the docked panels and the activity bar at the end of the list.
    Item {
        id: panelSpace

        width: 1
        height: (view.selecting || App.clipboardCount > 0 ? Theme.itemSizeLarge : 0) + activityBar.space
    }

    // The loading, empty and error states sit in the content of the list or
    // grid, as Silica's ViewPlaceholder does, so pulling the pulley moves
    // them with the content. As siblings of the views they stayed put, and in
    // an empty folder they are all there is below the header.
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

    // Above the paste bar; out of the way while selecting (XFR-13).
    ActivityBar {
        id: activityBar

        allowed: !view.selecting
        anchors.bottomMargin: clipboardPanel.visibleSize
    }
}
