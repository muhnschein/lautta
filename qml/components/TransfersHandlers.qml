// SPDX-License-Identifier: LGPL-2.1-or-later
// App-wide reactions of the transfers area: keep the device awake while
// transfers run (XFR-5), notifications (INT-2), conflict questions
// (OPS-2) and the restored-transfers prompt (XFR-11).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Nemo.KeepAlive 1.2
import Lautta 1.0

Item {
    id: root

    // Conflict dialogs waiting for their turn, and the one on screen.
    property var queue: []
    property var asked: ({})
    property bool dialogOpen
    // The restored-transfers prompt is shown once per start.
    property bool restoredShown

    function showTransfers() {
        __silica_applicationwindow_instance.activate()
        var top = pageStack.currentPage
        if (top && top.objectName === "transfersPage")
            return
        pageStack.push(Qt.resolvedUrl("../pages/TransfersPage.qml"))
    }

    // A conflict of a running transfer: the operations area's dialog answers
    // it through Transfers.answer. One dialog at a time.
    function ask(transferId, item, conflict) {
        var key = transferId + ":" + item
        if (asked[key])
            return
        asked[key] = true
        queue.push({ "key": key, "props": { "transferId": transferId, "item": item, "conflict": conflict } })
        next()
    }

    function next() {
        if (dialogOpen || queue.length === 0)
            return
        var entry = queue.shift()
        var dialog = pageStack.push(Qt.resolvedUrl("../dialogs/ConflictDialog.qml"), entry.props)
        if (!dialog) {
            delete asked[entry.key]
            return
        }
        dialogOpen = true
        var done = function() {
            delete asked[entry.key]
            dialogOpen = false
            next()
        }
        dialog.accepted.connect(done)
        dialog.rejected.connect(done)
    }

    function askFirstOpen(transferId) {
        var open = JSON.parse(Transfers.questionsJson(transferId))
        if (open.length > 0)
            ask(transferId, open[0].item, open[0])
        else
            showTransfers()
    }

    function openFromNotification(page, transferId) {
        if (page === "conflict")
            askFirstOpen(transferId)
        else
            showTransfers()
        __silica_applicationwindow_instance.activate()
    }

    KeepAlive {
        enabled: Transfers.busy
    }

    TransferNotifications {
        id: notifications

        onOpenRequested: root.openFromNotification(page, transferId)
    }

    Connections {
        target: Transfers

        onNeedsAnswer: root.ask(id, item, JSON.parse(conflictJson))
        onShowRequested: root.showTransfers()
        onChanged: {
            if (Transfers.pendingAtStart > 0 && !root.restoredShown) {
                root.restoredShown = true
                root.showTransfers()
            }
        }
    }

    Connections {
        target: Qt.application

        onAboutToQuit: {
            Transfers.noteClosing()
            notifications.publishPending()
        }
    }
}
