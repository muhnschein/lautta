// SPDX-License-Identifier: LGPL-2.1-or-later Remote files copied to ~/Downloads/Lautta/Opened
// first; local go as is. Not visible: call openWith() or share().
import QtQuick 2.6
import Sailfish.Silica 1.0
import Sailfish.Share 1.0
import Lautta 1.0

Item {
    id: root

    property string uri
    property bool busy
    property string pending

    signal failed(string kind, string message)

    function openWith() {
        run("open")
    }

    function share() {
        run("share")
    }

    function run(what) {
        if (App.isLocal(uri)) {
            deliver(what, App.localUrl(uri))
        } else if (!busy) {
            busy = true
            pending = what
            App.prepareExternal(uri)
        }
    }

    function deliver(what, fileUrl) {
        if (what === "share") {
            shareAction.resources = [fileUrl]
            shareAction.mimeType = mimeForViewer(App.viewerFor(uri, ""))
            shareAction.trigger()
        } else {
            tools.noteOpened(uri)
            Qt.openUrlExternally(fileUrl)
        }
    }

    function mimeForViewer(viewer) {
        switch (viewer) {
        case "image": return "image/*"
        case "text": return "text/plain"
        case "markdown": return "text/markdown"
        case "audio": return "audio/*"
        case "video": return "video/*"
        case "pdf": return "application/pdf"
        default: return "application/octet-stream"
        }
    }

    Connections {
        target: App
        onExternalReady: {
            if (root.busy && uri === root.uri) {
                root.busy = false
                root.deliver(root.pending, fileUrl)
            }
        }
        onExternalFailed: {
            if (root.busy && uri === root.uri) {
                root.busy = false
                root.failed(kind, message)
            }
        }
    }

    ShareAction { id: shareAction }

    ViewerTools { id: tools }
}
