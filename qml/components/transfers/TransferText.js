// SPDX-License-Identifier: LGPL-2.1-or-later
// Texts of the transfers area: titles, status lines and reasons, built from
// the raw numbers and engineering names the Transfers models give (QML
// formats sizes and times, doc/QML-API.md).
.pragma library
.import Sailfish.Silica 1.0 as Silica

function size(bytes) {
    return Silica.Format.formatFileSize(bytes)
}

function duration(seconds) {
    return Silica.Format.formatDuration(seconds, Silica.Formatter.DurationShort)
}

function when(ms) {
    return Silica.Format.formatDate(new Date(ms), Silica.Formatter.TimepointRelativeCurrentDay)
}

function percent(done, total) {
    return total > 0 ? Math.min(100, Math.floor(done * 100 / total)) : 0
}

// "Copy 37 items to NAS", "Upload figure-3.png to NAS", "Download holiday.mp4".
// r: { kind, direction, state, title, itemsTotal, destName }
function title(r) {
    var one = r.itemsTotal <= 1
    var past = r.state === "completed"
    switch (r.kind) {
    case "copy":
        if (one && r.direction === "upload")
            //% "Upload %1 to %2"
            return qsTrId("lautta-xfr-title-upload-one").arg(r.title).arg(r.destName)
        if (one && r.direction === "download")
            //% "Download %1"
            return qsTrId("lautta-xfr-title-download-one").arg(r.title)
        if (one)
            return past
                //% "Copied %1 to %2"
                ? qsTrId("lautta-xfr-title-copied-one").arg(r.title).arg(r.destName)
                //% "Copy %1 to %2"
                : qsTrId("lautta-xfr-title-copy-one").arg(r.title).arg(r.destName)
        return past
            //% "Copied %n item(s) to %1"
            ? qsTrId("lautta-xfr-title-copied-many", r.itemsTotal).arg(r.destName)
            //% "Copy %n item(s) to %1"
            : qsTrId("lautta-xfr-title-copy-many", r.itemsTotal).arg(r.destName)
    case "move":
        if (one)
            return past
                //% "Moved %1 to %2"
                ? qsTrId("lautta-xfr-title-moved-one").arg(r.title).arg(r.destName)
                //% "Move %1 to %2"
                : qsTrId("lautta-xfr-title-move-one").arg(r.title).arg(r.destName)
        return past
            //% "Moved %n item(s) to %1"
            ? qsTrId("lautta-xfr-title-moved-many", r.itemsTotal).arg(r.destName)
            //% "Move %n item(s) to %1"
            : qsTrId("lautta-xfr-title-move-many", r.itemsTotal).arg(r.destName)
    case "delete":
        if (one)
            //% "Delete %1"
            return qsTrId("lautta-xfr-title-delete-one").arg(r.title)
        //% "Delete %n item(s)"
        return qsTrId("lautta-xfr-title-delete-many", r.itemsTotal)
    case "compress":
        if (one)
            //% "Compress %1"
            return qsTrId("lautta-xfr-title-compress-one").arg(r.title)
        //% "Compress %n item(s)"
        return qsTrId("lautta-xfr-title-compress-many", r.itemsTotal)
    case "extract":
        //% "Extract %1 to %2"
        return qsTrId("lautta-xfr-title-extract").arg(r.title).arg(r.destName)
    case "sync":
        //% "Sync %n item(s) to %1"
        return qsTrId("lautta-xfr-title-sync", r.itemsTotal).arg(r.destName)
    default:
        //% "Upload %1 to %2"
        return qsTrId("lautta-xfr-title-upload-one").arg(r.title).arg(r.destName)
    }
}

// "1.2 GB of 4.0 GB · 11.4 MB/s · 4 min"
function progressLine(done, total, rate, eta) {
    //% "%1 of %2"
    var text = qsTrId("lautta-xfr-progress").arg(size(done)).arg(size(total))
    if (rate > 0)
        //% "%1/s"
        text += " · " + qsTrId("lautta-xfr-rate").arg(size(rate))
    if (rate > 0 && eta >= 0)
        text += " · " + duration(eta)
    return text
}

// "about 4 min left" for the details page.
function etaLine(rate, eta) {
    if (rate <= 0)
        return ""
    var text = qsTrId("lautta-xfr-rate").arg(size(rate))
    if (eta >= 0)
        //% "about %1 left"
        text += " · " + qsTrId("lautta-xfr-eta-left").arg(duration(eta))
    return text
}

function waitReason(reason) {
    switch (reason) {
    case "network":
        //% "Waiting for network"
        return qsTrId("lautta-xfr-wait-network")
    case "volume":
        //% "Insert the card to continue"
        return qsTrId("lautta-xfr-wait-volume")
    case "bridge":
        //% "Waiting for the connection"
        return qsTrId("lautta-xfr-wait-bridge")
    default:
        //% "Waiting for your answer"
        return qsTrId("lautta-xfr-wait-question")
    }
}

function needsAnswer(questions) {
    //% "Needs an answer: %n conflict(s)"
    return qsTrId("lautta-xfr-needs-answer", questions)
}

// The second line of a transfer row. r is a TransfersModel row.
// restored: add "resumes from where it stopped" to paused rows.
function status(r, restored) {
    switch (r.group) {
    case "active":
        if (r.state === "scanning")
            //% "Scanning… %n item(s)"
            return qsTrId("lautta-xfr-scanning", r.itemsTotal)
        if (r.state === "queued" && r.bytesDone === 0)
            //% "Queued"
            return qsTrId("lautta-xfr-queued")
        return progressLine(r.bytesDone, r.bytesTotal, r.rate, r.eta)
    case "waiting":
        return waitReason(r.waitReason)
    case "paused":
        //% "Paused at %1%"
        var paused = qsTrId("lautta-xfr-paused-at").arg(percent(r.bytesDone, r.bytesTotal))
        //% "resumes from where it stopped"
        return restored ? paused + " · " + qsTrId("lautta-xfr-resumes") : paused
    case "edited":
        if (r.dirty)
            //% "Changed · upload pending"
            return qsTrId("lautta-xfr-edit-changed")
        if (r.lastUploadMs >= 0)
            //% "Uploaded %1 · watching"
            return qsTrId("lautta-xfr-edit-uploaded").arg(when(r.lastUploadMs))
        //% "Watching for changes"
        return qsTrId("lautta-xfr-edit-watching")
    default:
        return historyLine(r)
    }
}

function historyLine(r) {
    var at = r.finishedMs >= 0 ? when(r.finishedMs) : ""
    var text
    if (r.state === "canceled")
        //% "Canceled"
        text = qsTrId("lautta-xfr-canceled")
    else if (r.state === "failed" && r.itemsFailed > 0 && r.itemsDone > 0)
        //% "Completed with %n failure(s)"
        text = qsTrId("lautta-xfr-completed-failures", r.itemsFailed)
    else if (r.state === "failed")
        //% "Failed"
        text = qsTrId("lautta-xfr-failed")
    else
        //% "Completed"
        text = qsTrId("lautta-xfr-completed")
    return at.length > 0 ? text + " · " + at : text
}

// Name of the edited file with where it lives: "budget.ods · Office".
function editedTitle(r) {
    //% "%1 · %2"
    return qsTrId("lautta-xfr-edit-title").arg(r.title).arg(r.destName)
}

function directionIcon(direction) {
    switch (direction) {
    case "upload":
        return "image://theme/icon-m-cloud-upload"
    case "download":
        return "image://theme/icon-m-cloud-download"
    case "delete":
        return "image://theme/icon-m-delete"
    case "edit":
        return "image://theme/icon-m-edit"
    default:
        return "image://theme/icon-m-transfer"
    }
}

function itemIcon(state) {
    switch (state) {
    case "done":
        return "image://theme/icon-m-acknowledge"
    case "failed":
        return "image://theme/icon-m-warning"
    case "skipped":
        return "image://theme/icon-m-dismiss"
    case "needs-answer":
        return "image://theme/icon-m-question"
    case "running":
        return "image://theme/icon-m-transfer"
    default:
        return "image://theme/icon-m-clock"
    }
}

// "Uploading · 6.8 MB", "Queued · 5.1 MB", "Completed · 7.2 MB" (details page).
function itemStatus(state, kind, sizeBytes, direction) {
    var label
    switch (state) {
    case "done":
        //% "Completed"
        label = qsTrId("lautta-xfr-completed")
        break
    case "running":
        label = direction === "upload"
            //% "Uploading"
            ? qsTrId("lautta-xfr-item-uploading")
            : direction === "download"
                //% "Downloading"
                ? qsTrId("lautta-xfr-item-downloading")
                //% "Copying"
                : qsTrId("lautta-xfr-item-copying")
        break
    case "skipped":
        //% "Skipped"
        label = qsTrId("lautta-xfr-item-skipped")
        break
    case "needs-answer":
        //% "Needs an answer"
        label = qsTrId("lautta-xfr-item-needs-answer")
        break
    default:
        //% "Queued"
        label = qsTrId("lautta-xfr-queued")
    }
    return kind === "dir" || sizeBytes <= 0 ? label : label + " · " + size(sizeBytes)
}
