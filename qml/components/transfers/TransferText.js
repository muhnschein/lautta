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

// "Copy 37 items to NAS", "Upload figure-3.png to NAS", "Download holiday.mp4";
// completed ones in the past tense ("Deleted 4 items").
// r: { kind, direction, state, title, itemsTotal, destName }
function title(r) {
    if (r.state === "completed")
        return ended(r)
    var one = r.itemsTotal <= 1
    switch (r.kind) {
    case "copy":
        if (one && r.direction === "upload")
            //% "Upload %1 to %2"
            return qsTrId("lautta-xfr-title-upload-one").arg(r.title).arg(r.destName)
        if (one && r.direction === "download")
            //% "Download %1"
            return qsTrId("lautta-xfr-title-download-one").arg(r.title)
        if (one)
            //% "Copy %1 to %2"
            return qsTrId("lautta-xfr-title-copy-one").arg(r.title).arg(r.destName)
        //% "Copy %n item(s) to %1"
        return qsTrId("lautta-xfr-title-copy-many", r.itemsTotal).arg(r.destName)
    case "move":
        if (one)
            //% "Move %1 to %2"
            return qsTrId("lautta-xfr-title-move-one").arg(r.title).arg(r.destName)
        //% "Move %n item(s) to %1"
        return qsTrId("lautta-xfr-title-move-many", r.itemsTotal).arg(r.destName)
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
    default:
        return qsTrId("lautta-xfr-title-upload-one").arg(r.title).arg(r.destName)
    }
}

// "Copied 37 items to NAS", "Deleted notes.md", "Extracted dataset.tar.gz".
function ended(r) {
    var one = r.itemsTotal <= 1
    switch (r.kind) {
    case "copy":
        if (one && r.direction === "upload")
            //% "Uploaded %1 to %2"
            return qsTrId("lautta-xfr-title-uploaded-one").arg(r.title).arg(r.destName)
        if (one && r.direction === "download")
            //% "Downloaded %1"
            return qsTrId("lautta-xfr-title-downloaded-one").arg(r.title)
        if (one)
            //% "Copied %1 to %2"
            return qsTrId("lautta-xfr-title-copied-one").arg(r.title).arg(r.destName)
        //% "Copied %n item(s) to %1"
        return qsTrId("lautta-xfr-title-copied-many", r.itemsTotal).arg(r.destName)
    case "move":
        if (one)
            //% "Moved %1 to %2"
            return qsTrId("lautta-xfr-title-moved-one").arg(r.title).arg(r.destName)
        //% "Moved %n item(s) to %1"
        return qsTrId("lautta-xfr-title-moved-many", r.itemsTotal).arg(r.destName)
    case "delete":
        if (one)
            //% "Deleted %1"
            return qsTrId("lautta-xfr-title-deleted-one").arg(r.title)
        //% "Deleted %n item(s)"
        return qsTrId("lautta-xfr-title-deleted-many", r.itemsTotal)
    case "compress":
        if (one)
            //% "Compressed %1"
            return qsTrId("lautta-xfr-title-compressed-one").arg(r.title)
        //% "Compressed %n item(s)"
        return qsTrId("lautta-xfr-title-compressed-many", r.itemsTotal)
    case "extract":
        //% "Extracted %1 to %2"
        return qsTrId("lautta-xfr-title-extracted").arg(r.title).arg(r.destName)
    default:
        return qsTrId("lautta-xfr-title-uploaded-one").arg(r.title).arg(r.destName)
    }
}

// "Copying 37 items to NAS", "Deleting notes.md": what runs now.
function ongoing(r) {
    var one = r.itemsTotal <= 1
    switch (r.kind) {
    case "copy":
        if (one && r.direction === "upload")
            //% "Uploading %1 to %2"
            return qsTrId("lautta-xfr-title-uploading-one").arg(r.title).arg(r.destName)
        if (one && r.direction === "download")
            //% "Downloading %1"
            return qsTrId("lautta-xfr-title-downloading-one").arg(r.title)
        if (one)
            //% "Copying %1 to %2"
            return qsTrId("lautta-xfr-title-copying-one").arg(r.title).arg(r.destName)
        //% "Copying %n item(s) to %1"
        return qsTrId("lautta-xfr-title-copying-many", r.itemsTotal).arg(r.destName)
    case "move":
        if (one)
            //% "Moving %1 to %2"
            return qsTrId("lautta-xfr-title-moving-one").arg(r.title).arg(r.destName)
        //% "Moving %n item(s) to %1"
        return qsTrId("lautta-xfr-title-moving-many", r.itemsTotal).arg(r.destName)
    case "delete":
        if (one)
            //% "Deleting %1"
            return qsTrId("lautta-xfr-title-deleting-one").arg(r.title)
        //% "Deleting %n item(s)"
        return qsTrId("lautta-xfr-title-deleting-many", r.itemsTotal)
    case "compress":
        if (one)
            //% "Compressing %1"
            return qsTrId("lautta-xfr-title-compressing-one").arg(r.title)
        //% "Compressing %n item(s)"
        return qsTrId("lautta-xfr-title-compressing-many", r.itemsTotal)
    case "extract":
        //% "Extracting %1 to %2"
        return qsTrId("lautta-xfr-title-extracting").arg(r.title).arg(r.destName)
    default:
        return qsTrId("lautta-xfr-title-uploading-one").arg(r.title).arg(r.destName)
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

function directionIcon(direction) {
    switch (direction) {
    case "upload":
        return "image://theme/icon-m-cloud-upload"
    case "download":
        return "image://theme/icon-m-cloud-download"
    case "delete":
        return "image://theme/icon-m-delete"
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

// The activity bar (XFR-13). mode is Transfers.barMode, s the lead transfer's
// summary (Transfers.summaryJson), others how many more are unfinished.
function barTitle(mode, s) {
    if (mode === "done")
        return ended(s)
    if (mode === "failed")
        return title(s)
    var text = ongoing(s)
    return mode === "running" && s.bytesTotal > 0 ? text + " · " + percent(s.bytesDone, s.bytesTotal) + "%" : text
}

// "120 MB of 290 MB · 1 min · 2 more", "Waiting for your answer", "Completed with 1 failure".
function barDetail(mode, s, others, questions) {
    var text
    switch (mode) {
    case "question":
        text = needsAnswer(Math.max(1, questions))
        break
    case "waiting":
        text = waitReason(s.waitReason)
        break
    case "paused":
        text = qsTrId("lautta-xfr-paused-at").arg(percent(s.bytesDone, s.bytesTotal))
        break
    case "running":
        if (s.state === "scanning")
            text = qsTrId("lautta-xfr-scanning", s.itemsTotal)
        else if (s.bytesTotal > 0)
            text = progressLine(s.bytesDone, s.bytesTotal, s.rate, s.eta)
        else
            //% "%1 of %2 items"
            text = qsTrId("lautta-xfr-items-progress").arg(s.itemsDone).arg(s.itemsTotal)
        break
    default:
        return historyLine({ "state": s.state, "itemsFailed": s.itemsFailed, "itemsDone": s.itemsDone, "finishedMs": -1 })
    }
    //% "%n more"
    return others > 0 ? text + " · " + qsTrId("lautta-xfr-bar-more", others) : text
}

function barIcon(mode, direction) {
    switch (mode) {
    case "done":
        return "image://theme/icon-m-acknowledge"
    case "failed":
    case "question":
        return "image://theme/icon-m-warning"
    case "paused":
        return "image://theme/icon-m-pause"
    default:
        return directionIcon(direction)
    }
}
