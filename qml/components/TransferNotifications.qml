// SPDX-License-Identifier: LGPL-2.1-or-later
// Transfer notifications (SPEC INT-2, XFR-6, XFR-8), shown while the app is
// in the background: one updating progress notification, finished transfers
// grouped in one, one per failed transfer, one per transfer that needs an
// answer, and "will resume" when the app closes with work pending.
//
// Default actions: the running app reacts to `clicked` (openRequested). For a
// closed app the notification carries a D-Bus action on the app's own name
// (org.netvfs.lautta, SPEC §3.3): it starts the app through ExecDBus. Showing
// a specific page after such a start needs a D-Bus object in the app, which
// this release does not export, so the app comes up on its main page.
import QtQuick 2.6
import Nemo.Notifications 1.0
import Lautta 1.0
import "transfers/TransferText.js" as TransferText

Item {
    id: root

    // The user tapped a notification while the app runs: page is
    // "transfers" or "conflict" (then transferId says which transfer).
    signal openRequested(string page, int transferId)
    // For page "edit" the second argument is the working copy's id.

    readonly property bool inBackground: Qt.application.state !== Qt.ApplicationActive
    property int finishedCount
    // Notifications per transfer id (needs-answer), reused when it asks again.
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
        //% "%n transfer(s) · %1%"
        var text = qsTrId("lautta-xfr-note-progress", Transfers.activeCount).arg(percent)
        return Transfers.eta >= 0 ? text + " · " + TransferText.duration(Transfers.eta) : text
    }

    function publishProgress() {
        progressNote.summary = progressSummary()
        progressNote.progress = Transfers.bytesTotal > 0 ? Transfers.bytesDone / Transfers.bytesTotal : -1
        progressNote.publish()
    }

    // Called when the app is closing (XFR-6): what is still unfinished
    // resumes the next time the app opens.
    function publishPending() {
        progressNote.close()
        if (Transfers.pendingCount > 0) {
            //% "%n transfer(s) will resume"
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
            //% "%n file(s) couldn't be transferred. Tap to retry."
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
        //% "%n conflict(s) in %1"
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
        //% "Transfers finished"
        summary: qsTrId("lautta-xfr-note-finished")
        //% "%n transfer(s)"
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
