// SPDX-License-Identifier: LGPL-2.1-or-later Closed app: notification carries a D-Bus action on
// org.netvfs.lautta that starts the app through ExecDBus. App exports no D-Bus object, so it comes
// up on its main page.
import QtQuick 2.6
import Nemo.Notifications 1.0
import Lautta 1.0
import "transfers/TransferText.js" as TransferText

Item {
    id: root

    signal openRequested(string page, int transferId)

    readonly property bool inBackground: Qt.application.state !== Qt.ApplicationActive
    property int finishedCount
    property var questionNotes: ({})

    function remoteAction(page) {
        return [{
            "name": "default",
            "service": "org.netvfs.lautta",
            "path": "/org/netvfs/lautta",
            "iface": "org.netvfs.lautta",
            "method": "openPage",
            "arguments": [page]
        }]
    }

    function summaryOf(id) {
        var text = Transfers.summaryJson(id)
        return text.length > 0 ? JSON.parse(text) : null
    }

    function titleOf(id) {
        var s = summaryOf(id)
        return s ? TransferText.title(s) : ""
    }

    function progressSummary() {
        var percent = TransferText.percent(Transfers.bytesDone, Transfers.bytesTotal)
        //% "%n tasks · %1%"
        var text = qsTrId("lautta-xfr-note-progress", Transfers.activeCount).arg(percent)
        return Transfers.eta >= 0 ? text + " · " + TransferText.duration(Transfers.eta) : text
    }

    function publishProgress() {
        progressNote.summary = progressSummary()
        progressNote.progress = Transfers.bytesTotal > 0 ? Transfers.bytesDone / Transfers.bytesTotal : -1
        progressNote.publish()
    }

    function publishPending() {
        progressNote.close()
        if (Transfers.pendingCount > 0) {
            //% "%n tasks will resume"
            pendingNote.summary = qsTrId("lautta-xfr-note-pending", Transfers.pendingCount)
            pendingNote.publish()
        }
    }

    function closeStale() {
        var stale = pendingNote.notificationsByCategory("x-lautta.transfer.pending")
        for (var i = 0; i < stale.length; ++i)
            stale[i].close()
    }

    function noteFinished(id, ok, failures) {
        if (!inBackground)
            return
        if (ok) {
            finishedCount += 1
            doneNote.publish()
            return
        }
        var failedNote = failedComponent.createObject(root, {
            "transferId": id,
            //% "%1 failed"
            "summary": qsTrId("lautta-xfr-note-failed").arg(titleOf(id)),
            //% "%n files couldn't be transferred. Tap to retry."
            "body": qsTrId("lautta-xfr-note-failed-body", Math.max(1, failures))
        })
        failedNote.publish()
    }

    function noteQuestion(id) {
        if (!inBackground)
            return
        var count = JSON.parse(Transfers.questionsJson(id)).length
        var note = questionNotes[id]
        if (!note) {
            note = questionComponent.createObject(root, { "transferId": id })
            questionNotes[id] = note
        }
        //% "%n conflicts in %1"
        note.body = qsTrId("lautta-xfr-note-question-body", Math.max(1, count)).arg(titleOf(id))
        note.publish()
    }

    Component.onCompleted: closeStale()

    Connections {
        target: Transfers
        onFinished: root.noteFinished(id, ok, failures)
        onNeedsAnswer: root.noteQuestion(id)
    }

    Timer {
        interval: 2000
        repeat: true
        running: root.inBackground && Transfers.busy
        triggeredOnStart: true
        onTriggered: root.publishProgress()
        onRunningChanged: {
            if (!running)
                progressNote.close()
        }
    }

    Notification {
        id: progressNote

        appName: "Lautta"
        category: "x-lautta.transfer.progress"
        resident: true
        isTransient: false
        remoteActions: root.remoteAction("transfers")
        onClicked: root.openRequested("transfers", 0)
    }

    Notification {
        id: doneNote

        appName: "Lautta"
        category: "x-lautta.transfer.finished"
        //% "Tasks finished"
        summary: qsTrId("lautta-xfr-note-finished")
        //% "%n tasks"
        body: qsTrId("lautta-xfr-note-finished-count", root.finishedCount)
        itemCount: root.finishedCount
        remoteActions: root.remoteAction("transfers")
        onClicked: root.openRequested("transfers", 0)
        onClosed: root.finishedCount = 0
    }

    Notification {
        id: pendingNote

        appName: "Lautta"
        category: "x-lautta.transfer.pending"
        //% "when you open Lautta again"
        body: qsTrId("lautta-xfr-note-pending-body")
        remoteActions: root.remoteAction("transfers")
    }

    Component {
        id: failedComponent

        Notification {
            property int transferId

            appName: "Lautta"
            category: "x-lautta.transfer.failed"
            remoteActions: root.remoteAction("transfers")
            onClicked: root.openRequested("transfers", transferId)
            onClosed: destroy()
        }
    }

    Component {
        id: questionComponent

        Notification {
            property int transferId

            appName: "Lautta"
            category: "x-lautta.transfer.question"
            //% "Lautta needs an answer"
            summary: qsTrId("lautta-xfr-note-question")
            remoteActions: root.remoteAction("conflict")
            onClicked: root.openRequested("conflict", transferId)
        }
    }
}
