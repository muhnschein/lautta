// SPDX-License-Identifier: LGPL-2.1-or-later
// The folder view of DirectoryPage (SPEC §9, §15.3): header with the path menu, list or grid, pulley,
// context menu, selection mode with a docked panel, paste bar, remorse and
// the placeholders for empty, not accessible, offline, large and error.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Sailfish.Share 1.0
import Lautta 1.0
import "../"
import "../Open.js" as Open
import "../ErrorText.js" as ErrorText
import "ContextActions.js" as Actions
import "DirFormat.js" as DirFormat

Item {
    id: view

    property string uri
    property bool selectionMode
    property bool detailsShown
    property alias model: dir
    readonly property bool selecting: selectionMode
    readonly property bool gridMode: dir.viewMode === "grid"
    readonly property bool hasRemote: pickerRoots.hasRemote()
    readonly property bool permanentDelete: !(App.setting("recently_deleted") === true && dir.hasCapability("Trash")
                                              && App.isLocal(uri))
    property string pendingName
    property string pendingShare

    // Asks the host page to open a folder.
    signal openFolder(string uri)

    function endSelection() {
        dir.clearSelection()
        selectionMode = false
    }

    function openItem(itemUri, isDir, mimeType) {
        if (isDir)
            openFolder(itemUri)
        else
            Open.open(pageStack, App, Operations, itemUri, false, mimeType, uri)
    }

    function newItem() {
        var dialog = pageStack.push(Qt.resolvedUrl("../../dialogs/NewItemDialog.qml"), { "parentUri": uri })
        dialog.accepted.connect(function () {
            view.pendingName = dialog.itemName
            if (dialog.isFolder)
                dir.createFolder(dialog.itemName)
            else
                dir.createFile(dialog.itemName)
        })
    }

    function rename(info) {
        var dialog = pageStack.push(Qt.resolvedUrl("../../dialogs/RenameDialog.qml"),
                                    { "uri": info.uri, "size": info.size, "modified": info.modified, "isDir": info.isDir })
        dialog.accepted.connect(function () {
            view.pendingName = dialog.newName
            dir.rename(info.uri, dialog.newName)
        })
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

    function pickFolder(uris, kind) {
        var count = uris.length
        var title
        var accept
        if (kind === "move") {
            //% "Move %n items to"
            title = qsTrId("lautta-dir-picker-move", count)
            //% "Move here"
            accept = qsTrId("lautta-dir-picker-move-here")
        } else if (kind === "copy") {
            //% "Copy %n items to"
            title = qsTrId("lautta-dir-picker-copy", count)
            //% "Copy here"
            accept = qsTrId("lautta-dir-picker-copy-here")
        } else if (kind === "download") {
            //% "Download %n items to"
            title = qsTrId("lautta-dir-picker-download", count)
            //% "Download here"
            accept = qsTrId("lautta-dir-picker-download-here")
        } else {
            //% "Upload %n items to"
            title = qsTrId("lautta-dir-picker-upload", count)
            //% "Upload here"
            accept = qsTrId("lautta-dir-picker-upload-here")
        }
        var dialog = pageStack.push(Qt.resolvedUrl("../../dialogs/FolderPickerDialog.qml"),
                                    { "title": title, "acceptText": accept, "startUri": uri })
        dialog.accepted.connect(function () {
            if (kind === "move")
                Operations.moveTo(JSON.stringify(uris), dialog.selectedUri)
            else
                Operations.copyTo(JSON.stringify(uris), dialog.selectedUri)
        })
    }

    function remove(uris, listItem) {
        var timeout = App.setting("remorse_seconds") * 1000
        var action = function () {
            Operations.remove(JSON.stringify(uris))
            refreshTimer.restart()
            if (view.selecting)
                view.endSelection()
        }
        if (listItem) {
            //% "Deleting permanently"
            var single = view.permanentDelete ? qsTrId("lautta-dir-deleting-permanently")
                                              //% "Deleting"
                                              : qsTrId("lautta-dir-deleting")
            listItem.remorseAction(single, action, timeout)
        } else {
            //% "Deleting %n items permanently"
            var many = view.permanentDelete ? qsTrId("lautta-dir-deleting-many-permanently", uris.length)
                                            //% "Deleting %n items"
                                            : qsTrId("lautta-dir-deleting-many", uris.length)
            remorsePopup.execute(many, action, timeout)
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

    function openWith(itemUri) {
        if (App.isLocal(itemUri))
            Qt.openUrlExternally(App.localUrl(itemUri))
        else
            pageStack.push(Qt.resolvedUrl("../../dialogs/OpenRemoteDialog.qml"), { "uri": itemUri })
    }

    // One action on one or more items; `info` describes the first item.
    function runAction(id, uris, info, listItem) {
        switch (id) {
        case "open_with": openWith(uris[0]); break
        case "share": share(uris, info.mimeType); break
        case "copy": App.copy(JSON.stringify(uris)); if (selecting) endSelection(); break
        case "cut": App.cut(JSON.stringify(uris)); if (selecting) endSelection(); break
        case "rename": rename(info); break
        case "delete": remove(uris, listItem); break
        case "copy_to": pickFolder(uris, "copy"); break
        case "move_to": pickFolder(uris, "move"); break
        case "download": pickFolder(uris, "download"); break
        case "upload_to": pickFolder(uris, "upload"); break
        case "info": pageStack.push(Qt.resolvedUrl("../../pages/InfoPage.qml"), { "uri": uris[0] }); break
        case "compress": pageStack.push(Qt.resolvedUrl("../../dialogs/CompressDialog.qml"), { "uris": JSON.stringify(uris) }); break
        case "extract": pageStack.push(Qt.resolvedUrl("../../dialogs/ExtractDialog.qml"), { "archiveUri": uris[0] }); break
        case "tags": pageStack.push(Qt.resolvedUrl("../../dialogs/TagAssignDialog.qml"), { "uris": JSON.stringify(uris) }); break
        case "favourite": pageStack.push(Qt.resolvedUrl("../../dialogs/FavouriteDialog.qml"), { "uri": uris[0], "favouriteId": "" }); break
        case "open_remote": pageStack.push(Qt.resolvedUrl("../../dialogs/OpenRemoteDialog.qml"), { "uri": uris[0] }); break
        }
    }

    // Actions of the selection panel (board DirectorySelect).
    function runSelectionAction(id) {
        var uris = dir.selectedUris()
        if (uris.length === 0)
            return
        var info = { "uri": uris[0], "mimeType": "", "isDir": false }
        switch (id) {
        case "rename_all": pageStack.push(Qt.resolvedUrl("../../pages/BulkRenamePage.qml"), { "uris": JSON.stringify(uris) }); break
        default: runAction(id, uris, info, null)
        }
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
            pageStack.push(Qt.resolvedUrl("../../pages/LocationSettingsPage.qml"), { "locationId": uri.split("/")[2] })
            break
        }
    }

    function notifyFailure(kind, message, item) {
        notice.show(ErrorText.message(kind, { "location": dir.locationName, "item": item }))
    }

    DirectoryModel {
        id: dir

        uri: view.uri
        onRenamed: App.noteUndo()
        onRenameFailed: view.notifyFailure(kind, message, view.pendingName)
        onCreateFailed: view.notifyFailure(kind, message, view.pendingName)
        onSelectedCountChanged: {
            if (selectedCount > 0)
                view.selectionMode = true
            else if (view.selectionMode && !view.pulleySelect)
                view.selectionMode = false
        }
    }

    PathModel {
        id: path

        uri: view.uri
    }

    PickerRootsModel {
        id: pickerRoots
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

    // True while the pulley's "Select" waits for the first tap.
    readonly property bool pulleySelect: selectionMode && dir.selectedCount === 0

    Component {
        id: headerComponent

        Column {
            width: view.width

            Item {
                width: parent.width
                height: pageHeader.height

                PageHeader {
                    id: pageHeader

                    title: view.selecting
                           //% "%n selected"
                           ? qsTrId("lautta-dir-selected", dir.selectedCount)
                           : dir.title
                    description: view.selecting ? dir.title : path.breadcrumb
                }
                MouseArea {
                    anchors.fill: parent
                    enabled: !view.selecting
                    onClicked: pathMenu.open = !pathMenu.open
                }
            }

            SlimProgress {
                running: dir.loading
            }

            PathMenu {
                id: pathMenu

                pathModel: path
                currentUri: view.uri
                onNavigate: view.openFolder(uri)
                onInfoRequested: view.runAction("info", [view.uri], {}, null)
                onFavouriteRequested: view.runAction("favourite", [view.uri], {}, null)
            }

            InfoBanner {
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
                visible: dir.large
                height: visible ? implicitHeight : 0
                //% "%1 loaded · large folder: name order, thumbnails off"
                text: qsTrId("lautta-dir-large").arg(dir.count)
                //% "Stop"
                actionText: dir.loading ? qsTrId("lautta-dir-large-stop") : ""
                onAction: dir.stopLoading()
            }

            InfoBanner {
                visible: dir.suggestGrid
                height: visible ? implicitHeight : 0
                //% "Shown as grid because most items are photos"
                text: qsTrId("lautta-dir-grid-suggested")
                //% "Use list"
                actionText: qsTrId("lautta-dir-use-list")
                onAction: dir.setViewPrefs(JSON.stringify({ "viewMode": "list" }))
            }

            InfoBanner {
                visible: dir.errorKind.length > 0 && dir.count > 0 && !dir.offline
                height: visible ? implicitHeight : 0
                text: ErrorText.message(dir.errorKind, { "location": dir.locationName, "item": dir.title })
                actionText: qsTrId("lautta-dir-retry")
                onAction: dir.refresh()
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
                if (view.selecting)
                    dir.toggle(model.uri)
            }

            Rectangle {
                anchors.fill: parent
                visible: model.selected
                color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity / 2)
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

                // Tapping the icon selects (design: DirectorySelect).
                MouseArea {
                    anchors {
                        fill: parent
                        margins: -Theme.paddingMedium
                    }
                    onClicked: {
                        dir.toggle(model.uri)
                        view.selectionMode = true
                    }
                }
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

            width: grid.cellWidth
            height: grid.cellHeight
            onClicked: {
                if (view.selecting)
                    dir.toggle(model.uri)
                else
                    view.openItem(model.uri, model.isDir, model.mimeType)
            }
            onPressAndHold: {
                dir.toggle(model.uri)
                view.selectionMode = true
            }

            FileIcon {
                anchors {
                    fill: parent
                    margins: Theme.paddingSmall / 2
                }
                size: Math.min(width, height)
                category: model.category
                isDir: model.isDir
                isSymlink: model.isSymlink
                thumbnailSource: model.thumbnailSource
                mimeType: model.mimeType
                selected: model.selected
            }

            Label {
                visible: model.thumbnailSource.length === 0
                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                    margins: Theme.paddingSmall
                }
                horizontalAlignment: Text.AlignHCenter
                text: model.name
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeExtraSmall
            }
        }
    }

    SilicaListView {
        id: list

        anchors.fill: parent
        visible: !view.gridMode
        model: view.gridMode ? null : dir
        header: headerComponent
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

        readonly property int columns: Math.max(3, Math.round(width / (Theme.itemSizeExtraLarge * 1.1)))

        anchors.fill: parent
        visible: view.gridMode
        model: view.gridMode ? dir : null
        cellWidth: width / columns
        cellHeight: cellWidth
        header: headerComponent
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

    // Room for the docked panels at the end of the list.
    Item {
        id: panelSpace

        width: 1
        height: view.selecting || App.clipboardCount > 0 ? Theme.itemSizeLarge : 0
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: dir.loading && dir.count === 0 && dir.errorKind.length === 0 && view.visible
    }

    FolderPlaceholder {
        id: placeholder

        readonly property string kind: view.placeholderKind()

        visible: kind.length > 0
        x: Theme.horizontalPageMargin
        y: Math.round(parent.height * 0.38)
        title: view.placeholderTitle(kind)
        hint: view.placeholderHint(kind)
        buttons: view.placeholderButtons(kind)
        onTriggered: view.placeholderAction(action)
    }

    SelectionPanel {
        owner: view
    }

    ClipboardPanel {
        owner: view
    }
}
