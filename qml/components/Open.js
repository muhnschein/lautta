// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

var viewerPages = {
    "image": "../viewers/ImageViewer.qml",
    "text": "../viewers/TextViewer.qml",
    "markdown": "../viewers/MarkdownViewer.qml",
    "audio": "../viewers/MediaPlayer.qml",
    "video": "../viewers/MediaPlayer.qml",
    "pdf": "../viewers/PdfViewer.qml"
}

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
    openWith(pageStack, app, uri)
    return "external"
}

function openWith(pageStack, app, uri) {
    if (app.isLocal(uri))
        Qt.openUrlExternally(app.localUrl(uri))
    else
        pageStack.push(Qt.resolvedUrl("../dialogs/OpenRemoteDialog.qml"), { "uri": uri })
}
