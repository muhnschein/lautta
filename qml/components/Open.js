// SPDX-License-Identifier: LGPL-2.1-or-later
// Opens an item: folders and archives browse, viewable files get their
// viewer page (PRV-4), everything else goes to the system handler (PRV-5,
// remote files are copied out first, PRV-6).
.pragma library

var viewerPages = {
    "image": "../viewers/ImageViewer.qml",
    "text": "../viewers/TextViewer.qml",
    "markdown": "../viewers/MarkdownViewer.qml",
    "audio": "../viewers/MediaPlayer.qml",
    "video": "../viewers/MediaPlayer.qml"
}

// pageStack: the window's page stack; app: the App singleton;
// operations: the Operations singleton (archives); base: Qt.resolvedUrl of
// the calling file's folder ("components/").
function open(pageStack, app, operations, uri, isDir, mimeType, folderUri) {
    if (isDir) {
        pageStack.push(Qt.resolvedUrl("../pages/DirectoryPage.qml"), { "uri": uri })
        return "folder"
    }
    var viewer = app.viewerFor(uri, mimeType || "")
    if (viewer === "archive") {
        operations.openArchive(uri)
        return viewer
    }
    var page = viewerPages[viewer]
    if (page) {
        var props = { "uri": uri }
        if (viewer === "image")
            props.folderUri = folderUri || app.parentUri(uri)
        if (viewer === "audio" || viewer === "video")
            props.video = viewer === "video"
        pageStack.push(Qt.resolvedUrl(page), props)
        return viewer
    }
    if (app.isLocal(uri))
        Qt.openUrlExternally(app.localUrl(uri))
    else
        pageStack.push(Qt.resolvedUrl("../dialogs/OpenRemoteDialog.qml"), { "uri": uri })
    return "external"
}
