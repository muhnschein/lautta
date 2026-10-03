// SPDX-License-Identifier: LGPL-2.1-or-later
// Open with / Share for the file of a viewer (PRV-5, PRV-6, PRV-7): local
// files go to the system as they are, remote files are copied to
// ~/Downloads/Lautta/Opened first (SEC-4). Not visible; call openWith() or
// share().
import QtQuick 2.6
import Sailfish.Silica 1.0
import Sailfish.Share 1.0
import Lautta 1.0

Item {
    id: root

    property string uri
    // True while a remote copy is being made.
    property bool busy
    // "open" or "share": what to do when the copy is ready.
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
        case "sqlite": return "application/vnd.sqlite3"
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
